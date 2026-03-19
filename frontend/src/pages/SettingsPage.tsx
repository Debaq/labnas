import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { QRCodeSVG } from 'qrcode.react'
import { Palette, HardDrive, Info, Power, Loader2, MessageCircle, Plus, Trash2, Send, Clock, TerminalSquare, ExternalLink, X } from 'lucide-react'
import { useTheme } from '../themes/ThemeContext'
import { themes, type ThemeName } from '../themes/themes'
import { fetchDisks, fetchSystemInfo, fetchAutostartStatus, fetchNotificationConfig, addWhatsAppContact, deleteWhatsAppContact, sendTestWhatsApp, setNotificationSchedule } from '../api'
import type { DiskInfo, SystemInfo, AutostartStatus, NotificationConfig } from '../types'

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}

function ThemeCard({
  name,
  isSelected,
  onClick,
}: {
  name: ThemeName
  isSelected: boolean
  onClick: () => void
}) {
  const t = themes[name]
  return (
    <button
      onClick={onClick}
      className="rounded-xl p-4 transition-all duration-200 hover:shadow-lg hover:-translate-y-0.5 text-left"
      style={{
        backgroundColor: t['bg-primary'],
        border: isSelected ? `2px solid var(--accent)` : `2px solid ${t['border']}`,
      }}
    >
      <div className="flex items-center gap-2 mb-3">
        <span className="text-sm font-semibold" style={{ color: t['text-primary'] }}>
          {name.charAt(0).toUpperCase() + name.slice(1)}
        </span>
        {isSelected && (
          <span
            className="text-xs px-2 py-0.5 rounded-full font-medium"
            style={{ backgroundColor: t.accent + '30', color: t.accent }}
          >
            Activo
          </span>
        )}
      </div>
      <div className="flex gap-1.5 mb-3">
        <span className="w-6 h-6 rounded-full" style={{ backgroundColor: t['bg-primary'], border: `1px solid ${t.border}` }} />
        <span className="w-6 h-6 rounded-full" style={{ backgroundColor: t['sidebar-bg'], border: `1px solid ${t.border}` }} />
        <span className="w-6 h-6 rounded-full" style={{ backgroundColor: t.accent }} />
        <span className="w-6 h-6 rounded-full" style={{ backgroundColor: t.success }} />
        <span className="w-6 h-6 rounded-full" style={{ backgroundColor: t.danger }} />
      </div>
      <div className="flex gap-1">
        <span className="h-1.5 flex-1 rounded-full" style={{ backgroundColor: t.accent }} />
        <span className="h-1.5 flex-[2] rounded-full" style={{ backgroundColor: t['bg-tertiary'] }} />
        <span className="h-1.5 flex-1 rounded-full" style={{ backgroundColor: t.success }} />
      </div>
    </button>
  )
}

export default function SettingsPage() {
  const navigate = useNavigate()
  const { theme, setTheme, themeNames } = useTheme()
  const [disks, setDisks] = useState<DiskInfo[]>([])
  const [sysInfo, setSysInfo] = useState<SystemInfo | null>(null)
  const [autostart, setAutostart] = useState<AutostartStatus | null>(null)

  // WhatsApp
  const [notifConfig, setNotifConfig] = useState<NotificationConfig | null>(null)
  const [showAddContact, setShowAddContact] = useState(false)
  const [contactName, setContactName] = useState('')
  const [contactPhone, setContactPhone] = useState('')
  const [contactApiKey, setContactApiKey] = useState('')
  const [sendingTest, setSendingTest] = useState(false)
  const [qrExpanded, setQrExpanded] = useState(false)
  const [testResult, setTestResult] = useState<string | null>(null)
  const [scheduleHour, setScheduleHour] = useState(8)
  const [scheduleMinute, setScheduleMinute] = useState(0)
  const [dailyEnabled, setDailyEnabled] = useState(false)

  useEffect(() => {
    fetchDisks().then(setDisks).catch(() => {})
    fetchSystemInfo().then(setSysInfo).catch(() => {})
    fetchAutostartStatus().then(setAutostart).catch(() => {})
    fetchNotificationConfig().then((c) => {
      setNotifConfig(c)
      setDailyEnabled(c.daily_enabled)
      setScheduleHour(c.daily_hour)
      setScheduleMinute(c.daily_minute)
    }).catch(() => {})
  }, [])

  function handleAutostartTerminal(install: boolean) {
    if (!autostart) return
    const cmd = install ? autostart.install_cmd : autostart.uninstall_cmd
    navigate('/terminal', { state: { commands: cmd } })
  }

  return (
    <div className="space-y-8 max-w-4xl">
      {/* Appearance */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <Palette size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Apariencia
          </h2>
        </div>
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          {themeNames.map((t) => (
            <ThemeCard
              key={t}
              name={t}
              isSelected={t === theme}
              onClick={() => setTheme(t)}
            />
          ))}
        </div>
      </section>

      {/* Autostart */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <Power size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Inicio Automatico
          </h2>
        </div>
        <div
          className="rounded-xl p-6"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <div className="flex items-center justify-between mb-3">
            <div>
              <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                Iniciar LabNAS con el sistema
              </p>
              <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
                Instala un servicio systemd. Se abrira la terminal para ingresar la contrasena sudo.
              </p>
            </div>
            {autostart && (
              <div className="flex items-center gap-2">
                <span
                  className="w-2 h-2 rounded-full"
                  style={{ backgroundColor: autostart.enabled ? 'var(--success)' : 'var(--text-secondary)' }}
                />
                <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                  {autostart.enabled ? 'Habilitado' : 'Deshabilitado'}
                </span>
              </div>
            )}
          </div>
          <div className="flex items-center gap-3">
            {!autostart?.enabled && (
              <button
                onClick={() => handleAutostartTerminal(true)}
                disabled={!autostart}
                className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
                style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
              >
                <TerminalSquare size={16} />
                Habilitar al inicio
              </button>
            )}
            {autostart?.enabled && (
              <button
                onClick={() => handleAutostartTerminal(false)}
                className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
                style={{ color: 'var(--danger)', border: '1px solid var(--danger)' }}
              >
                <TerminalSquare size={16} />
                Deshabilitar
              </button>
            )}
          </div>
        </div>
      </section>

      {/* WhatsApp Notifications */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <MessageCircle size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Notificaciones WhatsApp
          </h2>
        </div>

        {/* Info / Setup Guide */}
        <div
          className="rounded-xl p-5 mb-4"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <p className="text-sm font-medium mb-4" style={{ color: 'var(--text-primary)' }}>
            Como configurar CallMeBot
          </p>
          <div className="flex flex-col sm:flex-row gap-5">
            {/* QR Code */}
            <div className="flex flex-col items-center gap-2 shrink-0">
              <button
                onClick={() => setQrExpanded(true)}
                className="rounded-lg p-3 cursor-pointer transition-all duration-200 hover:shadow-lg hover:scale-105"
                style={{ backgroundColor: '#ffffff' }}
                title="Click para ampliar"
              >
                <QRCodeSVG
                  value="https://wa.me/34644457057?text=I%20allow%20callmebot%20to%20send%20me%20messages"
                  size={140}
                  level="M"
                />
              </button>
              <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>
                Toca para ampliar
              </span>
              <a
                href="https://wa.me/34644457057?text=I%20allow%20callmebot%20to%20send%20me%20messages"
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-80"
                style={{ color: 'var(--accent)', border: '1px solid var(--accent)' }}
              >
                <ExternalLink size={12} />
                Abrir en WhatsApp
              </a>
            </div>

            {/* QR Modal */}
            {qrExpanded && (
              <div
                className="fixed inset-0 z-50 flex items-center justify-center"
                style={{ backgroundColor: 'rgba(0,0,0,0.7)' }}
                onClick={() => setQrExpanded(false)}
              >
                <div
                  className="relative rounded-2xl p-8 shadow-2xl"
                  style={{ backgroundColor: '#ffffff' }}
                  onClick={(e) => e.stopPropagation()}
                >
                  <button
                    onClick={() => setQrExpanded(false)}
                    className="absolute top-3 right-3 p-1 rounded-full hover:opacity-70 transition-opacity"
                    style={{ color: '#666' }}
                  >
                    <X size={20} />
                  </button>
                  <QRCodeSVG
                    value="https://wa.me/34644457057?text=I%20allow%20callmebot%20to%20send%20me%20messages"
                    size={300}
                    level="M"
                  />
                  <p className="text-center text-sm mt-4" style={{ color: '#333' }}>
                    Escanea con tu celular
                  </p>
                </div>
              </div>
            )}
            {/* Steps */}
            <div className="space-y-3 text-xs" style={{ color: 'var(--text-secondary)' }}>
              <div className="flex gap-3">
                <span
                  className="shrink-0 w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold"
                  style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
                >1</span>
                <p>
                  Escanea el QR o toca el boton para abrir WhatsApp. Envia el mensaje
                  <span className="font-mono mx-1 px-1.5 py-0.5 rounded" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--accent)' }}>
                    I allow callmebot to send me messages
                  </span>
                  al numero <strong style={{ color: 'var(--text-primary)' }}>+34 644 45 70 57</strong>.
                </p>
              </div>
              <div className="flex gap-3">
                <span
                  className="shrink-0 w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold"
                  style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
                >2</span>
                <p>
                  CallMeBot te respondera con tu <strong style={{ color: 'var(--text-primary)' }}>API Key</strong> (un numero). Guardalo.
                </p>
              </div>
              <div className="flex gap-3">
                <span
                  className="shrink-0 w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold"
                  style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
                >3</span>
                <p>
                  Agrega el contacto abajo con su <strong style={{ color: 'var(--text-primary)' }}>nombre</strong>,
                  <strong style={{ color: 'var(--text-primary)' }}> telefono</strong> (con codigo pais, ej: 56912345678) y la
                  <strong style={{ color: 'var(--text-primary)' }}> API Key</strong>.
                </p>
              </div>
              <div className="flex gap-3">
                <span
                  className="shrink-0 w-5 h-5 rounded-full flex items-center justify-center text-[10px] font-bold"
                  style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
                >4</span>
                <p>
                  Usa <strong style={{ color: 'var(--text-primary)' }}>Enviar Test</strong> para verificar que funcione.
                  Activa el <strong style={{ color: 'var(--text-primary)' }}>mensaje diario</strong> y elige la hora.
                </p>
              </div>
            </div>
          </div>
        </div>

        {/* Contacts */}
        <div
          className="rounded-xl p-6 space-y-4"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <div className="flex items-center justify-between">
            <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
              Contactos ({notifConfig?.whatsapp_contacts.length ?? 0})
            </span>
            <div className="flex items-center gap-2">
              <button
                onClick={async () => {
                  setSendingTest(true)
                  setTestResult(null)
                  try {
                    const result = await sendTestWhatsApp()
                    setTestResult(result)
                  } catch (e: any) {
                    setTestResult(e.message)
                  } finally {
                    setSendingTest(false)
                  }
                }}
                disabled={sendingTest || !notifConfig?.whatsapp_contacts.length}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-90"
                style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
              >
                {sendingTest ? <Loader2 size={14} className="animate-spin" /> : <Send size={14} />}
                Enviar Test
              </button>
              <button
                onClick={() => setShowAddContact(true)}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-90"
                style={{ color: 'var(--accent)', border: '1px solid var(--accent)' }}
              >
                <Plus size={14} />
                Agregar
              </button>
            </div>
          </div>

          {testResult && (
            <div className="text-xs rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}>
              {testResult}
            </div>
          )}

          {/* Contact list */}
          {notifConfig?.whatsapp_contacts.map((c) => (
            <div key={c.phone} className="flex items-center justify-between py-2" style={{ borderTop: '1px solid var(--border)' }}>
              <div>
                <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{c.name}</span>
                <span className="text-xs font-mono ml-2" style={{ color: 'var(--text-secondary)' }}>{c.phone}</span>
              </div>
              <button
                onClick={async () => {
                  try {
                    await deleteWhatsAppContact(c.phone)
                    const cfg = await fetchNotificationConfig()
                    setNotifConfig(cfg)
                  } catch {}
                }}
                className="p-1.5 rounded-lg transition-all duration-200 hover:opacity-80"
                style={{ color: 'var(--danger)' }}
              >
                <Trash2 size={14} />
              </button>
            </div>
          ))}

          {/* Add contact form */}
          {showAddContact && (
            <div className="space-y-3 pt-3" style={{ borderTop: '1px solid var(--border)' }}>
              <div className="grid grid-cols-3 gap-2">
                <input
                  type="text"
                  value={contactName}
                  onChange={(e) => setContactName(e.target.value)}
                  placeholder="Nombre"
                  className="px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                />
                <input
                  type="text"
                  value={contactPhone}
                  onChange={(e) => setContactPhone(e.target.value)}
                  placeholder="56912345678"
                  className="px-3 py-2 rounded-lg text-sm outline-none font-mono"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                />
                <input
                  type="text"
                  value={contactApiKey}
                  onChange={(e) => setContactApiKey(e.target.value)}
                  placeholder="API Key"
                  className="px-3 py-2 rounded-lg text-sm outline-none font-mono"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                />
              </div>
              <div className="flex items-center gap-2">
                <button
                  onClick={async () => {
                    if (!contactName || !contactPhone || !contactApiKey) return
                    try {
                      await addWhatsAppContact({ name: contactName, phone: contactPhone, apikey: contactApiKey })
                      setContactName('')
                      setContactPhone('')
                      setContactApiKey('')
                      setShowAddContact(false)
                      const cfg = await fetchNotificationConfig()
                      setNotifConfig(cfg)
                    } catch {}
                  }}
                  className="px-3 py-1.5 rounded-lg text-xs font-medium"
                  style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
                >
                  Guardar
                </button>
                <button
                  onClick={() => setShowAddContact(false)}
                  className="px-3 py-1.5 rounded-lg text-xs font-medium"
                  style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
                >
                  Cancelar
                </button>
              </div>
            </div>
          )}
        </div>

        {/* Schedule */}
        <div
          className="rounded-xl p-6 mt-4"
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
                  Envia un resumen del sistema a todos los contactos
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
      </section>

      {/* Storage */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <HardDrive size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Almacenamiento
          </h2>
        </div>
        <div className="space-y-3">
          {disks.length === 0 ? (
            <div
              className="rounded-xl p-6"
              style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
            >
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                Cargando discos...
              </span>
            </div>
          ) : (
            disks.map((disk) => (
              <div
                key={disk.mount_point}
                className="rounded-xl p-6"
                style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
              >
                <div className="flex items-center justify-between mb-3">
                  <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>
                    {disk.name || disk.mount_point}
                  </span>
                  <span className="text-xs font-mono px-2 py-0.5 rounded" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}>
                    {disk.file_system}
                  </span>
                </div>
                <div className="w-full h-2 rounded-full mb-2" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                  <div
                    className="h-full rounded-full transition-all duration-300"
                    style={{
                      width: `${disk.total_space > 0 ? (disk.used_space / disk.total_space) * 100 : 0}%`,
                      backgroundColor: (disk.used_space / disk.total_space) > 0.9 ? 'var(--danger)' : 'var(--accent)',
                    }}
                  />
                </div>
                <div className="flex items-center justify-between text-xs" style={{ color: 'var(--text-secondary)' }}>
                  <span>{formatBytes(disk.used_space)} usado de {formatBytes(disk.total_space)}</span>
                  <span>{formatBytes(disk.available_space)} disponible</span>
                </div>
                <div className="mt-2 text-xs font-mono" style={{ color: 'var(--text-secondary)' }}>
                  Montado en: {disk.mount_point}
                </div>
              </div>
            ))
          )}
        </div>
      </section>

      {/* About */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <Info size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Acerca de
          </h2>
        </div>
        <div
          className="rounded-xl p-6 space-y-3"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <div className="flex items-center justify-between">
            <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
              Version
            </span>
            <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
              LabNAS v0.2.3
            </span>
          </div>
          {sysInfo && (
            <>
              <div className="flex items-center justify-between">
                <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>Host</span>
                <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{sysInfo.hostname}</span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>IP</span>
                <span className="text-sm font-medium font-mono" style={{ color: 'var(--accent)' }}>{sysInfo.local_ip}</span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>SO</span>
                <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{sysInfo.os}</span>
              </div>
              <div className="flex items-center justify-between">
                <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>Kernel</span>
                <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{sysInfo.kernel}</span>
              </div>
            </>
          )}
          <div
            className="pt-3"
            style={{ borderTop: '1px solid var(--border)' }}
          >
            <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
              NAS para laboratorio - Archivos, impresoras 3D, impresion CUPS, red y terminal
            </p>
          </div>
        </div>
      </section>
    </div>
  )
}
