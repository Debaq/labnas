import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Palette, HardDrive, Info, Power, Loader2, MessageCircle, Trash2, Send, Clock, TerminalSquare, Bot, Key, Users, ShieldCheck, ShieldAlert, UserCheck, Link2, Globe, Building2 } from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import { useTheme } from '../themes/ThemeContext'
import { themes, getThemeNames, type ThemeName } from '../themes/themes'
import { fetchDisks, fetchSystemInfo, fetchAutostartStatus, fetchNotificationConfig, setBotToken, deleteBotToken, deleteTelegramChat, sendTestTelegram, setNotificationSchedule, setChatRole, adminLinkChat, fetchWebUsers, generateLinkCode, changePassword, checkUpdate, doUpdate, getMdnsStatus, setMdns, getBranding, setBranding, type LabBranding } from '../api'
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
  const { user: authUser } = useAuth()
  const [disks, setDisks] = useState<DiskInfo[]>([])
  const [sysInfo, setSysInfo] = useState<SystemInfo | null>(null)
  const [webUsers, setWebUsers] = useState<string[]>([])
  const [linkCode, setLinkCode] = useState<string | null>(null)
  const [updateInfo, setUpdateInfo] = useState<{ current_version: string; latest_version: string | null; update_available: boolean } | null>(null)
  const [updating, setUpdating] = useState(false)
  const [mdns, setMdnsState] = useState<{ enabled: boolean; hostname: string; url: string } | null>(null)
  const [mdnsHostname, setMdnsHostname] = useState('')
  const [branding, setBrandingState] = useState<LabBranding | null>(null)
  const [currentPw, setCurrentPw] = useState('')
  const [newPw, setNewPw] = useState('')
  const [pwMsg, setPwMsg] = useState<{ ok: boolean; text: string } | null>(null)
  const [autostart, setAutostart] = useState<AutostartStatus | null>(null)

  // Telegram
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
    fetchDisks().then(setDisks).catch(() => {})
    fetchSystemInfo().then(setSysInfo).catch(() => {})
    fetchAutostartStatus().then(setAutostart).catch(() => {})
    fetchWebUsers().then(users => setWebUsers(users.map(u => u.username))).catch(() => {})
    checkUpdate().then(setUpdateInfo).catch(() => {})
    getMdnsStatus().then(s => { setMdnsState(s); setMdnsHostname(s.hostname) }).catch(() => {})
    getBranding().then(setBrandingState).catch(() => {})
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
      {/* Lab Branding (admin) */}
      {authUser?.role === 'admin' && branding && (
        <section>
          <div className="flex items-center gap-3 mb-4">
            <Building2 size={22} style={{ color: 'var(--accent)' }} />
            <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
              Identidad del Laboratorio
            </h2>
          </div>
          <div
            className="rounded-xl p-6 space-y-4"
            style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
          >
            <div className="grid grid-cols-1 sm:grid-cols-2 gap-4">
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Nombre del laboratorio</label>
                <input value={branding.lab_name} onChange={e => setBrandingState({ ...branding, lab_name: e.target.value })}
                  placeholder="FabLab Valdivia" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Institucion</label>
                <input value={branding.institution} onChange={e => setBrandingState({ ...branding, institution: e.target.value })}
                  placeholder="Universidad Austral de Chile" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>URL del logo</label>
                <input value={branding.logo_url} onChange={e => setBrandingState({ ...branding, logo_url: e.target.value })}
                  placeholder="https://ejemplo.com/logo.png" className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Sitio web</label>
                <input value={branding.website} onChange={e => setBrandingState({ ...branding, website: e.target.value })}
                  placeholder="https://fablab.uach.cl" className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Email de contacto</label>
                <input value={branding.contact_email} onChange={e => setBrandingState({ ...branding, contact_email: e.target.value })}
                  placeholder="fablab@uach.cl" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Ubicacion</label>
                <input value={branding.location} onChange={e => setBrandingState({ ...branding, location: e.target.value })}
                  placeholder="Valdivia, Chile" className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              </div>
            </div>
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Mision</label>
              <textarea value={branding.mission} onChange={e => setBrandingState({ ...branding, mission: e.target.value })}
                rows={2} placeholder="Promover la fabricacion digital y la innovacion..."
                className="w-full px-3 py-2 rounded-lg text-sm outline-none resize-none"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
            </div>
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Vision</label>
              <textarea value={branding.vision} onChange={e => setBrandingState({ ...branding, vision: e.target.value })}
                rows={2} placeholder="Ser un referente en fabricacion digital en la region..."
                className="w-full px-3 py-2 rounded-lg text-sm outline-none resize-none"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
            </div>
            {/* Color principal */}
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Color principal (accent)</label>
              <div className="flex items-center gap-3">
                <input
                  type="color"
                  value={branding.accent_color || '#bd93f9'}
                  onChange={e => setBrandingState({ ...branding, accent_color: e.target.value })}
                  className="w-10 h-10 rounded-lg cursor-pointer border-0 p-0"
                  style={{ backgroundColor: 'var(--input-bg)' }}
                />
                <span className="text-xs font-mono" style={{ color: 'var(--text-secondary)' }}>
                  {branding.accent_color || 'Sin personalizar (usa el del tema)'}
                </span>
                {branding.accent_color && (
                  <button
                    onClick={() => {
                      setBrandingState({ ...branding, accent_color: '' })
                      localStorage.removeItem('labnas-accent-color')
                      // Reaplicar tema sin accent personalizado
                      const currentTheme = localStorage.getItem('labnas-theme') || 'dracula'
                      const validThemes = getThemeNames()
                      const resolved = validThemes.includes(currentTheme as ThemeName) ? currentTheme as ThemeName : 'dracula'
                      const themeColors = themes[resolved]
                      document.documentElement.style.setProperty('--accent', themeColors.accent)
                      document.documentElement.style.setProperty('--accent-alpha', themeColors['accent-alpha'])
                      document.documentElement.style.setProperty('--sidebar-active', themeColors['sidebar-active'])
                    }}
                    className="text-xs px-2 py-1 rounded-lg transition-opacity hover:opacity-80"
                    style={{ color: 'var(--danger)', border: '1px solid var(--danger)' }}
                  >
                    Quitar
                  </button>
                )}
              </div>
            </div>

            <button
              onClick={async () => {
                try {
                  await setBranding(branding)
                  // Aplicar accent color inmediatamente
                  if (branding.accent_color) {
                    localStorage.setItem('labnas-accent-color', branding.accent_color)
                    document.documentElement.style.setProperty('--accent', branding.accent_color)
                    document.documentElement.style.setProperty('--accent-alpha', branding.accent_color + '1f')
                    document.documentElement.style.setProperty('--sidebar-active', branding.accent_color)
                  } else {
                    localStorage.removeItem('labnas-accent-color')
                  }
                } catch {}
              }}
              className="px-4 py-2 rounded-lg text-sm font-medium"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              Guardar
            </button>
          </div>
        </section>
      )}

      {/* Appearance */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <Palette size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Apariencia
          </h2>
        </div>
        <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-4 gap-4">
          {/* Auto theme card */}
          <button
            onClick={() => setTheme('auto' as any)}
            className="rounded-xl p-4 transition-all duration-200 hover:shadow-lg hover:-translate-y-0.5 text-left"
            style={{
              background: 'linear-gradient(135deg, #282a36 50%, #f8fafc 50%)',
              border: theme === 'auto' ? '2px solid var(--accent)' : '2px solid var(--border)',
            }}
          >
            <div className="flex items-center gap-2 mb-3">
              <span className="text-sm font-semibold" style={{ color: '#f8f8f2' }}>
                Automatico
              </span>
              {theme === 'auto' && (
                <span
                  className="text-xs px-2 py-0.5 rounded-full font-medium"
                  style={{ backgroundColor: 'var(--accent)' + '30', color: 'var(--accent)' }}
                >
                  Activo
                </span>
              )}
            </div>
            <p className="text-[10px]" style={{ color: '#6272a4' }}>
              Sigue el tema del sistema
            </p>
          </button>
          {/* Theme cards - solo los temas reales, no "auto" */}
          {themeNames.filter(t => t !== 'auto').map((t) => (
            <ThemeCard
              key={t}
              name={t as ThemeName}
              isSelected={t === theme}
              onClick={() => setTheme(t)}
            />
          ))}
        </div>
      </section>

      {/* Change password */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <Key size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Cambiar Contrasena
          </h2>
        </div>
        <div
          className="rounded-xl p-6"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <div className="flex items-end gap-3 flex-wrap">
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Actual</label>
              <input
                type="password"
                value={currentPw}
                onChange={e => { setCurrentPw(e.target.value); setPwMsg(null) }}
                className="px-3 py-2 rounded-lg text-sm outline-none w-44"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
              />
            </div>
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Nueva</label>
              <input
                type="password"
                value={newPw}
                onChange={e => { setNewPw(e.target.value); setPwMsg(null) }}
                className="px-3 py-2 rounded-lg text-sm outline-none w-44"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
              />
            </div>
            <button
              onClick={async () => {
                if (!currentPw || !newPw) return
                try {
                  await changePassword(currentPw, newPw)
                  setPwMsg({ ok: true, text: 'Contrasena cambiada' })
                  setCurrentPw('')
                  setNewPw('')
                } catch (e: any) {
                  setPwMsg({ ok: false, text: e.message })
                }
              }}
              disabled={!currentPw || !newPw}
              className="px-4 py-2 rounded-lg text-sm font-medium"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              Cambiar
            </button>
          </div>
          {pwMsg && (
            <p className="text-xs mt-3" style={{ color: pwMsg.ok ? 'var(--success)' : 'var(--danger)' }}>
              {pwMsg.text}
            </p>
          )}
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

      {/* Tailscale - Remote Access (admin only) */}
      {authUser?.role === 'admin' && (
        <section>
          <div className="flex items-center gap-3 mb-4">
            <Globe size={22} style={{ color: 'var(--accent)' }} />
            <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
              Acceso Remoto (Tailscale)
            </h2>
          </div>
          <div
            className="rounded-xl p-6"
            style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
          >
            <p className="text-xs mb-4" style={{ color: 'var(--text-secondary)' }}>
              Tailscale permite acceder a LabNAS desde cualquier parte del mundo de forma segura, sin exponer puertos a internet.
              Funciona en Linux, Windows, macOS, iPhone y Android.
            </p>
            <div className="flex items-center gap-3">
              <button
                onClick={() => navigate('/terminal', { state: { commands: 'curl -fsSL https://tailscale.com/install.sh | sh && sudo systemctl enable --now tailscaled && sudo tailscale up' } })}
                className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
                style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
              >
                <TerminalSquare size={16} />
                Instalar Tailscale
              </button>
              <button
                onClick={() => navigate('/terminal', { state: { commands: 'tailscale status' } })}
                className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
                style={{ color: 'var(--accent)', border: '1px solid var(--accent)' }}
              >
                Ver estado
              </button>
            </div>
          </div>
        </section>
      )}

      {/* mDNS - Local domain (admin only) */}
      {authUser?.role === 'admin' && (
        <section>
          <div className="flex items-center gap-3 mb-4">
            <Globe size={22} style={{ color: 'var(--accent)' }} />
            <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
              Nombre Local (.local)
            </h2>
          </div>
          <div
            className="rounded-xl p-6"
            style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
          >
            <p className="text-xs mb-4" style={{ color: 'var(--text-secondary)' }}>
              Permite acceder a LabNAS como <strong style={{ color: 'var(--accent)' }}>{mdnsHostname || 'labnas'}.local:3001</strong> desde cualquier dispositivo en la red, sin recordar la IP.
            </p>
            <div className="flex items-center gap-3 flex-wrap">
              <input
                type="text"
                value={mdnsHostname}
                onChange={e => setMdnsHostname(e.target.value.toLowerCase().replace(/[^a-z0-9-]/g, ''))}
                placeholder="labnas"
                className="px-3 py-2 rounded-lg text-sm outline-none font-mono w-40"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
              />
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>.local:3001</span>
              <button
                onClick={async () => {
                  try {
                    const result = await setMdns(!mdns?.enabled, mdnsHostname)
                    setMdnsState(result)
                  } catch {}
                }}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{
                  backgroundColor: mdns?.enabled ? 'var(--danger)' + '20' : 'var(--accent)',
                  color: mdns?.enabled ? 'var(--danger)' : '#ffffff',
                  border: mdns?.enabled ? '1px solid var(--danger)' : 'none',
                }}
              >
                {mdns?.enabled ? 'Desactivar' : 'Activar'}
              </button>
              {mdns?.enabled && (
                <span className="text-xs px-2 py-0.5 rounded-full font-medium" style={{ backgroundColor: 'var(--success)' + '25', color: 'var(--success)' }}>
                  Activo
                </span>
              )}
            </div>
          </div>
        </section>
      )}

      {/* Telegram Notifications */}
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
                    if (!authUser?.token) return
                    try {
                      const code = await generateLinkCode(authUser.token)
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
                    } catch (e: any) {
                      setTokenError(e.message)
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

        {/* Chats + Test */}
        {notifConfig?.bot_configured && (
          <div
            className="rounded-xl p-6 space-y-4 mb-4"
            style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
          >
            <div className="flex items-center justify-between">
              <div className="flex items-center gap-2">
                <Users size={16} style={{ color: 'var(--accent)' }} />
                <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                  Chats registrados ({notifConfig.telegram_chats.length})
                </span>
              </div>
              <button
                onClick={async () => {
                  setSendingTest(true)
                  setTestResult(null)
                  try {
                    const result = await sendTestTelegram()
                    setTestResult(result)
                  } catch (e: any) {
                    setTestResult(e.message)
                  } finally {
                    setSendingTest(false)
                  }
                }}
                disabled={sendingTest || !notifConfig.telegram_chats.length}
                className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-90"
                style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
              >
                {sendingTest ? <Loader2 size={14} className="animate-spin" /> : <Send size={14} />}
                Enviar Test
              </button>
            </div>

            {testResult && (
              <div className="text-xs rounded-lg p-3" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-secondary)' }}>
                {testResult}
              </div>
            )}

            {notifConfig.telegram_chats.length === 0 && (
              <div className="text-xs py-4 text-center" style={{ color: 'var(--text-secondary)' }}>
                Nadie se ha registrado aun. Los usuarios deben enviar <span className="font-mono px-1 rounded" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--accent)' }}>/start</span> al bot <strong style={{ color: 'var(--accent)' }}>@{notifConfig.bot_username}</strong> en Telegram.
              </div>
            )}

            {notifConfig.telegram_chats.map((c) => {
              const roleColor = c.role === 'admin' ? 'var(--accent)' : c.role === 'operador' ? 'var(--success)' : c.role === 'observador' ? 'var(--text-secondary)' : 'var(--warning)'
              const roleLabel = c.role === 'admin' ? 'Admin' : c.role === 'operador' ? 'Operador' : c.role === 'observador' ? 'Observador' : 'Pendiente'
              const RoleIcon = c.role === 'pendiente' ? ShieldAlert : ShieldCheck

              return (
                <div key={c.chat_id} className="py-3 space-y-2" style={{ borderTop: '1px solid var(--border)' }}>
                  <div className="flex items-center justify-between">
                    <div className="flex items-center gap-2">
                      <RoleIcon size={14} style={{ color: roleColor }} />
                      <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{c.name}</span>
                      {c.username && (
                        <span className="text-xs font-mono" style={{ color: 'var(--text-secondary)' }}>@{c.username}</span>
                      )}
                      <span className="text-xs px-1.5 py-0.5 rounded-full font-medium" style={{ backgroundColor: roleColor + '20', color: roleColor }}>
                        {roleLabel}
                      </span>
                      {c.linked_web_user && (
                        <span className="text-xs px-1.5 py-0.5 rounded flex items-center gap-1" style={{ backgroundColor: 'var(--success)' + '20', color: 'var(--success)' }}>
                          <Link2 size={10} />{c.linked_web_user}
                        </span>
                      )}
                      {c.daily_enabled && (
                        <span className="text-xs px-1.5 py-0.5 rounded" style={{ backgroundColor: 'var(--accent)' + '20', color: 'var(--accent)' }}>
                          {String(c.daily_hour).padStart(2, '0')}:{String(c.daily_minute).padStart(2, '0')}
                        </span>
                      )}
                    </div>
                    <button
                      onClick={async () => {
                        try {
                          await deleteTelegramChat(c.chat_id)
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

                  {/* Role controls */}
                  {c.role !== 'admin' && (
                    <div className="flex items-center gap-2 flex-wrap">
                      {c.role === 'pendiente' && (
                        <button
                          onClick={async () => {
                            try {
                              await setChatRole(c.chat_id, 'observador')
                              const cfg = await fetchNotificationConfig()
                              setNotifConfig(cfg)
                            } catch {}
                          }}
                          className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-medium"
                          style={{ color: 'var(--success)', border: '1px solid var(--success)' }}
                        >
                          <UserCheck size={12} />
                          Aprobar
                        </button>
                      )}
                      <select
                        value={c.role}
                        onChange={async (e) => {
                          try {
                            await setChatRole(c.chat_id, e.target.value, c.permissions)
                            const cfg = await fetchNotificationConfig()
                            setNotifConfig(cfg)
                          } catch {}
                        }}
                        className="px-2 py-1 rounded-lg text-xs outline-none cursor-pointer"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                      >
                        <option value="pendiente">Pendiente</option>
                        <option value="observador">Observador</option>
                        <option value="operador">Operador</option>
                      </select>
                      {(c.role === 'operador') && (
                        <div className="flex items-center gap-3 text-xs" style={{ color: 'var(--text-secondary)' }}>
                          <label className="flex items-center gap-1 cursor-pointer">
                            <input type="checkbox" checked={c.permissions.terminal}
                              onChange={async (e) => {
                                try {
                                  await setChatRole(c.chat_id, c.role, { ...c.permissions, terminal: e.target.checked })
                                  const cfg = await fetchNotificationConfig()
                                  setNotifConfig(cfg)
                                } catch {}
                              }} />
                            Terminal
                          </label>
                          <label className="flex items-center gap-1 cursor-pointer">
                            <input type="checkbox" checked={c.permissions.impresion}
                              onChange={async (e) => {
                                try {
                                  await setChatRole(c.chat_id, c.role, { ...c.permissions, impresion: e.target.checked })
                                  const cfg = await fetchNotificationConfig()
                                  setNotifConfig(cfg)
                                } catch {}
                              }} />
                            Impresion
                          </label>
                          <label className="flex items-center gap-1 cursor-pointer">
                            <input type="checkbox" checked={c.permissions.archivos_escritura}
                              onChange={async (e) => {
                                try {
                                  await setChatRole(c.chat_id, c.role, { ...c.permissions, archivos_escritura: e.target.checked })
                                  const cfg = await fetchNotificationConfig()
                                  setNotifConfig(cfg)
                                } catch {}
                              }} />
                            Archivos
                          </label>
                        </div>
                      )}
                      {/* Link to web user */}
                      <div className="flex items-center gap-2">
                        <Link2 size={12} style={{ color: 'var(--text-secondary)' }} />
                        <select
                          value={c.linked_web_user || ''}
                          onChange={async (e) => {
                            try {
                              if (e.target.value) {
                                await adminLinkChat(c.chat_id, e.target.value)
                              }
                              const cfg = await fetchNotificationConfig()
                              setNotifConfig(cfg)
                            } catch {}
                          }}
                          className="px-2 py-1 rounded-lg text-xs outline-none cursor-pointer"
                          style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                        >
                          <option value="">Sin vincular</option>
                          {webUsers.map(u => (
                            <option key={u} value={u}>{u}</option>
                          ))}
                        </select>
                      </div>
                    </div>
                  )}
                </div>
              )
            })}

            {/* Available commands reference */}
            <div className="pt-3" style={{ borderTop: '1px solid var(--border)' }}>
              <p className="text-xs font-medium mb-2" style={{ color: 'var(--text-secondary)' }}>Comandos disponibles en el bot:</p>
              <div className="grid grid-cols-2 sm:grid-cols-4 gap-1.5">
                {['/estado', '/discos', '/ram', '/cpu', '/uptime', '/red', '/impresoras', '/actividad', '/horario', '/ayuda'].map((cmd) => (
                  <span key={cmd} className="text-xs font-mono px-2 py-1 rounded text-center" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--accent)' }}>
                    {cmd}
                  </span>
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
            <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>Version</span>
            <div className="flex items-center gap-3">
              <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                v{updateInfo?.current_version || '?'}
              </span>
              {updateInfo?.update_available && (
                <span className="text-xs px-2 py-0.5 rounded-full font-medium" style={{ backgroundColor: 'var(--warning)' + '25', color: 'var(--warning)' }}>
                  {updateInfo.latest_version} disponible
                </span>
              )}
            </div>
          </div>
          {authUser?.role === 'admin' && updateInfo?.update_available && (
            <button
              onClick={async () => {
                if (!confirm('Actualizar LabNAS? El servidor se reiniciara.')) return
                setUpdating(true)
                try { await doUpdate() } catch {} finally { setUpdating(false) }
              }}
              disabled={updating}
              className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium w-full justify-center"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              {updating ? <Loader2 size={16} className="animate-spin" /> : null}
              {updating ? 'Actualizando...' : `Actualizar a ${updateInfo.latest_version}`}
            </button>
          )}
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
