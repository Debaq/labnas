import { useEffect, useState } from 'react'
import { Palette, HardDrive, Info, Power, Loader2 } from 'lucide-react'
import { useTheme } from '../themes/ThemeContext'
import { themes, type ThemeName } from '../themes/themes'
import { fetchDisks, fetchSystemInfo, fetchAutostartStatus, installAutostart, removeAutostart } from '../api'
import type { DiskInfo, SystemInfo, AutostartStatus } from '../types'

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
  const { theme, setTheme, themeNames } = useTheme()
  const [disks, setDisks] = useState<DiskInfo[]>([])
  const [sysInfo, setSysInfo] = useState<SystemInfo | null>(null)
  const [autostart, setAutostart] = useState<AutostartStatus | null>(null)
  const [autostartLoading, setAutostartLoading] = useState(false)
  const [autostartError, setAutostartError] = useState<string | null>(null)

  useEffect(() => {
    fetchDisks().then(setDisks).catch(() => {})
    fetchSystemInfo().then(setSysInfo).catch(() => {})
    fetchAutostartStatus().then(setAutostart).catch(() => {})
  }, [])

  async function handleToggleAutostart() {
    setAutostartLoading(true)
    setAutostartError(null)
    try {
      if (autostart?.enabled) {
        await removeAutostart()
      } else {
        await installAutostart()
      }
      const status = await fetchAutostartStatus()
      setAutostart(status)
    } catch (err: any) {
      setAutostartError(err.message || 'Error al configurar autostart')
    } finally {
      setAutostartLoading(false)
    }
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
          <div className="flex items-center justify-between">
            <div>
              <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                Iniciar LabNAS con el sistema
              </p>
              <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
                Instala un servicio systemd que inicia LabNAS automaticamente al arrancar
              </p>
            </div>
            <button
              onClick={handleToggleAutostart}
              disabled={autostartLoading}
              className="relative w-14 h-7 rounded-full transition-all duration-300 focus:outline-none"
              style={{
                backgroundColor: autostart?.enabled ? 'var(--accent)' : 'var(--bg-tertiary)',
                border: '1px solid var(--border)',
              }}
            >
              {autostartLoading ? (
                <div className="absolute inset-0 flex items-center justify-center">
                  <Loader2 size={14} className="animate-spin" style={{ color: 'var(--text-secondary)' }} />
                </div>
              ) : (
                <span
                  className="absolute top-0.5 w-5 h-5 rounded-full transition-all duration-300"
                  style={{
                    backgroundColor: autostart?.enabled ? '#ffffff' : 'var(--text-secondary)',
                    left: autostart?.enabled ? '30px' : '4px',
                  }}
                />
              )}
            </button>
          </div>
          {autostart && (
            <div className="flex items-center gap-4 mt-3 pt-3" style={{ borderTop: '1px solid var(--border)' }}>
              <div className="flex items-center gap-2">
                <span
                  className="w-2 h-2 rounded-full"
                  style={{ backgroundColor: autostart.installed ? 'var(--success)' : 'var(--text-secondary)' }}
                />
                <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                  Servicio {autostart.installed ? 'instalado' : 'no instalado'}
                </span>
              </div>
              <div className="flex items-center gap-2">
                <span
                  className="w-2 h-2 rounded-full"
                  style={{ backgroundColor: autostart.enabled ? 'var(--success)' : 'var(--text-secondary)' }}
                />
                <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                  {autostart.enabled ? 'Habilitado' : 'Deshabilitado'}
                </span>
              </div>
            </div>
          )}
          {autostartError && (
            <div className="mt-3 text-xs rounded-lg p-3" style={{ backgroundColor: 'var(--danger-alpha)', color: 'var(--danger)' }}>
              {autostartError}
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
