import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Power, TerminalSquare, HardDrive, Info, Loader2, RefreshCw, RotateCcw } from 'lucide-react'
import StorageRootsSection from '../StorageRootsSection'
import HttpsSection from '../HttpsSection'
import SmartSection from '../SmartSection'
import BridgeSection from '../BridgeSection'
import AiProviderSection from '../AiProviderSection'
import { fetchDisks, fetchSystemInfo, fetchAutostartStatus, checkUpdate, forceCheckUpdate, doUpdate, doReinstall, fetchRollbackStatus, doRollback, fetchHealth, setUploadLimit, type RollbackStatus } from '../../api'
import type { DiskInfo, SystemInfo, AutostartStatus } from '../../types'

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}

export default function SystemTab() {
  const navigate = useNavigate()
  const [disks, setDisks] = useState<DiskInfo[]>([])
  const [sysInfo, setSysInfo] = useState<SystemInfo | null>(null)
  const [updateInfo, setUpdateInfo] = useState<{ current_version: string; latest_version: string | null; update_available: boolean } | null>(null)
  const [updating, setUpdating] = useState(false)
  const [checking, setChecking] = useState(false)
  const [reinstalling, setReinstalling] = useState(false)
  const [rollback, setRollback] = useState<RollbackStatus | null>(null)
  const [rollingBack, setRollingBack] = useState(false)
  const [updateMsg, setUpdateMsg] = useState<{ ok: boolean; text: string } | null>(null)
  const [autostart, setAutostart] = useState<AutostartStatus | null>(null)

  useEffect(() => {
    fetchDisks().then(setDisks).catch(() => {})
    fetchSystemInfo().then(setSysInfo).catch(() => {})
    fetchAutostartStatus().then(setAutostart).catch(() => {})
    checkUpdate().then(setUpdateInfo).catch(() => {})
    fetchRollbackStatus().then(setRollback).catch(() => {})
  }, [])

  function handleAutostartTerminal(install: boolean) {
    if (!autostart) return
    const cmd = install ? autostart.install_cmd : autostart.uninstall_cmd
    navigate('/terminal', { state: { commands: cmd } })
  }

  return (
    <>
      {/* Autostart (admin only) */}
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

      {/* Storage (admin only) */}
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

      {/* About (admin only) */}
      <section id="actualizar" className="scroll-mt-4">
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
              <button
                onClick={async () => {
                  setChecking(true)
                  try { setUpdateInfo(await forceCheckUpdate()) } catch {} finally { setChecking(false) }
                }}
                disabled={checking}
                className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-medium transition-all hover:opacity-80"
                style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
                title="Buscar actualizaciones"
              >
                {checking ? <Loader2 size={12} className="animate-spin" /> : <RefreshCw size={12} />}
                {checking ? 'Buscando...' : 'Buscar'}
              </button>
            </div>
          </div>
          {updateInfo?.update_available && (
            <button
              onClick={async () => {
                if (!confirm('Actualizar LabNAS? El servidor se reiniciara.')) return
                setUpdating(true)
                setUpdateMsg(null)
                try { setUpdateMsg({ ok: true, text: await doUpdate() }) }
                catch (e) { setUpdateMsg({ ok: false, text: e instanceof Error ? e.message : String(e) }) }
                finally { setUpdating(false) }
              }}
              disabled={updating}
              className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium w-full justify-center"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              {updating ? <Loader2 size={16} className="animate-spin" /> : null}
              {updating ? 'Actualizando...' : `Actualizar a ${updateInfo.latest_version}`}
            </button>
          )}
          <button
            onClick={async () => {
              if (!confirm('Reinstalar la version actual? Se descargara y reemplazara el binario. El servidor se reiniciara.')) return
              setReinstalling(true)
              setUpdateMsg(null)
              try { setUpdateMsg({ ok: true, text: await doReinstall() }) }
              catch (e) { setUpdateMsg({ ok: false, text: e instanceof Error ? e.message : String(e) }) }
              finally { setReinstalling(false) }
            }}
            disabled={reinstalling}
            className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium w-full justify-center"
            style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
          >
            {reinstalling ? <Loader2 size={16} className="animate-spin" /> : <RefreshCw size={14} />}
            {reinstalling ? 'Reinstalando...' : 'Reinstalar version actual'}
          </button>
          {rollback?.available && (
            <button
              onClick={async () => {
                if (!confirm(`Volver a la version anterior (v${rollback.version})? El servidor se reiniciara.`)) return
                setRollingBack(true)
                setUpdateMsg(null)
                try { setUpdateMsg({ ok: true, text: await doRollback() }) }
                catch (e) { setUpdateMsg({ ok: false, text: e instanceof Error ? e.message : String(e) }) }
                finally { setRollingBack(false) }
              }}
              disabled={rollingBack}
              className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium w-full justify-center"
              style={{ color: 'var(--warning)', border: '1px solid var(--warning)' }}
            >
              {rollingBack ? <Loader2 size={16} className="animate-spin" /> : <RotateCcw size={14} />}
              {rollingBack ? 'Restaurando...' : `Volver a v${rollback.version}`}
            </button>
          )}
          {updateMsg && (
            <p className="text-sm" style={{ color: updateMsg.ok ? 'var(--success)' : 'var(--danger)' }}>{updateMsg.text}</p>
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
          <div className="flex items-center justify-between">
            <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>Limite de subida</span>
            <div className="flex items-center gap-2">
              <input type="number" min={1} max={500}
                defaultValue={updateInfo ? undefined : 50}
                value={undefined}
                ref={(el) => {
                  if (el && !el.dataset.init) {
                    fetchHealth().then(h => { el.value = String(h.upload_limit_mb || 50); el.dataset.init = '1' }).catch(() => {})
                  }
                }}
                onBlur={async (e) => {
                  const val = parseInt(e.target.value) || 50
                  try { await setUploadLimit(val) } catch {}
                }}
                className="w-16 px-2 py-1 rounded-lg text-sm text-right outline-none"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>MB</span>
            </div>
          </div>
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

      <StorageRootsSection />

      <HttpsSection />

      <SmartSection />

      <BridgeSection />
      <AiProviderSection />
    </>
  )
}
