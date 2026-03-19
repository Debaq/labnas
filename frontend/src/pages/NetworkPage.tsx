import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Radar, Monitor, Wifi, Loader2, Box, ShieldCheck, ShieldAlert, Tag, X } from 'lucide-react'
import { scanNetwork, fetchHosts, labelDevice, unlabelDevice } from '../api'
import type { NetworkHost } from '../types'

export default function NetworkPage() {
  const navigate = useNavigate()
  const [hosts, setHosts] = useState<NetworkHost[]>([])
  const [loading, setLoading] = useState(true)
  const [scanning, setScanning] = useState(false)
  const [labelModal, setLabelModal] = useState<NetworkHost | null>(null)
  const [labelInput, setLabelInput] = useState('')

  useEffect(() => {
    loadHosts()
  }, [])

  async function loadHosts() {
    setLoading(true)
    try {
      const data = await fetchHosts()
      setHosts(data)
    } catch {
      setHosts([])
    } finally {
      setLoading(false)
    }
  }

  async function handleScan() {
    setScanning(true)
    try {
      const data = await scanNetwork()
      setHosts(data)
    } catch {
    } finally {
      setScanning(false)
    }
  }

  async function handleLabel() {
    if (!labelModal?.mac || !labelInput.trim()) return
    try {
      await labelDevice(labelModal.mac, labelInput.trim())
      setLabelModal(null)
      setLabelInput('')
      const data = await fetchHosts()
      setHosts(data)
    } catch {}
  }

  async function handleUnlabel(mac: string) {
    try {
      await unlabelDevice(mac)
      const data = await fetchHosts()
      setHosts(data)
    } catch {}
  }

  const activeHosts = hosts.filter((h) => h.is_alive).length
  const unknownHosts = hosts.filter((h) => h.is_alive && !h.is_known).length

  return (
    <div className="space-y-6">
      {/* Stats & Scan */}
      <div className="flex items-center justify-between flex-wrap gap-4">
        <div className="flex items-center gap-6">
          <div className="flex items-center gap-2">
            <Monitor size={18} style={{ color: 'var(--text-secondary)' }} />
            <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
              Total: <strong style={{ color: 'var(--text-primary)' }}>{hosts.length}</strong>
            </span>
          </div>
          <div className="flex items-center gap-2">
            <Wifi size={18} style={{ color: 'var(--success)' }} />
            <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
              Activos: <strong style={{ color: 'var(--success)' }}>{activeHosts}</strong>
            </span>
          </div>
          {unknownHosts > 0 && (
            <div className="flex items-center gap-2">
              <ShieldAlert size={18} style={{ color: 'var(--warning)' }} />
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                Desconocidos: <strong style={{ color: 'var(--warning)' }}>{unknownHosts}</strong>
              </span>
            </div>
          )}
        </div>

        <button
          onClick={handleScan}
          disabled={scanning}
          className="flex items-center gap-2 px-5 py-2.5 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
          style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
        >
          {scanning ? <Loader2 size={18} className="animate-spin" /> : <Radar size={18} />}
          {scanning ? 'Escaneando...' : 'Escanear Red'}
        </button>
      </div>

      {/* Hosts Table */}
      <div
        className="rounded-xl overflow-hidden"
        style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
      >
        {loading || scanning ? (
          <div className="flex flex-col items-center justify-center py-16 gap-3">
            <Loader2 size={32} className="animate-spin" style={{ color: 'var(--accent)' }} />
            <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
              {scanning ? 'Escaneando la red...' : 'Cargando hosts...'}
            </p>
          </div>
        ) : hosts.length === 0 ? (
          <div className="text-center py-16">
            <Radar size={48} className="mx-auto mb-4" style={{ color: 'var(--text-secondary)' }} />
            <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
              No se encontraron dispositivos. Pulsa "Escanear Red" para buscar.
            </p>
          </div>
        ) : (
          <table className="w-full">
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border)' }}>
                <th className="text-left px-4 py-3 text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-secondary)' }}>Estado</th>
                <th className="text-left px-4 py-3 text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-secondary)' }}>IP</th>
                <th className="text-left px-4 py-3 text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-secondary)' }}>Dispositivo</th>
                <th className="text-left px-4 py-3 text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-secondary)' }}>MAC</th>
                <th className="text-left px-4 py-3 text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-secondary)' }}>Respuesta</th>
                <th className="text-right px-4 py-3 text-xs font-medium uppercase tracking-wider" style={{ color: 'var(--text-secondary)' }}>Acciones</th>
              </tr>
            </thead>
            <tbody>
              {hosts.map((host) => (
                <tr
                  key={host.ip}
                  className="transition-all duration-200 hover:opacity-90"
                  style={{
                    borderBottom: '1px solid var(--border)',
                    backgroundColor: !host.is_known && host.mac ? 'var(--warning)' + '08' : undefined,
                  }}
                >
                  {/* Status */}
                  <td className="px-4 py-3">
                    {host.is_known ? (
                      <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium" style={{ backgroundColor: 'var(--success)' + '20', color: 'var(--success)' }}>
                        <ShieldCheck size={12} />
                        Conocido
                      </span>
                    ) : host.mac ? (
                      <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium" style={{ backgroundColor: 'var(--warning)' + '20', color: 'var(--warning)' }}>
                        <ShieldAlert size={12} />
                        Nuevo
                      </span>
                    ) : (
                      <span className="inline-flex items-center gap-1 px-2 py-0.5 rounded-full text-xs font-medium" style={{ backgroundColor: 'var(--success)' + '20', color: 'var(--success)' }}>
                        <Wifi size={12} />
                        Activo
                      </span>
                    )}
                  </td>
                  {/* IP */}
                  <td className="px-4 py-3">
                    <span className="text-sm font-mono font-medium" style={{ color: 'var(--text-primary)' }}>
                      {host.ip}
                    </span>
                  </td>
                  {/* Device info */}
                  <td className="px-4 py-3">
                    <div>
                      {host.label && (
                        <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                          {host.label}
                        </span>
                      )}
                      {host.vendor && (
                        <span className={`text-xs ${host.label ? 'ml-2' : ''}`} style={{ color: 'var(--accent)' }}>
                          {host.vendor}
                        </span>
                      )}
                      {host.hostname && (
                        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                          {host.hostname}
                        </p>
                      )}
                      {!host.label && !host.vendor && !host.hostname && (
                        <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>--</span>
                      )}
                    </div>
                  </td>
                  {/* MAC */}
                  <td className="px-4 py-3">
                    <span className="text-xs font-mono" style={{ color: 'var(--text-secondary)' }}>
                      {host.mac || '--'}
                    </span>
                  </td>
                  {/* Response */}
                  <td className="px-4 py-3">
                    <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                      {host.response_time_ms != null ? `${host.response_time_ms} ms` : '--'}
                    </span>
                  </td>
                  {/* Actions */}
                  <td className="px-4 py-3">
                    <div className="flex items-center justify-end gap-1.5">
                      {host.mac && !host.is_known && (
                        <button
                          onClick={() => { setLabelModal(host); setLabelInput('') }}
                          className="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-80"
                          style={{ color: 'var(--success)', border: '1px solid var(--success)' }}
                          title="Marcar como conocido"
                        >
                          <Tag size={12} />
                          Etiquetar
                        </button>
                      )}
                      {host.is_known && host.mac && (
                        <button
                          onClick={() => handleUnlabel(host.mac!)}
                          className="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-80"
                          style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
                          title="Quitar etiqueta"
                        >
                          <X size={12} />
                        </button>
                      )}
                      {host.is_alive && (
                        <button
                          onClick={() => navigate(`/printers3d?ip=${host.ip}`)}
                          className="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-80"
                          style={{ color: 'var(--accent)', border: '1px solid var(--accent)' }}
                          title="Configurar como impresora 3D"
                        >
                          <Box size={12} />
                          3D
                        </button>
                      )}
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>

      {/* Label Modal */}
      {labelModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div
            className="rounded-xl p-6 w-full max-w-sm mx-4"
            style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
          >
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>Etiquetar dispositivo</h3>
              <button onClick={() => setLabelModal(null)} style={{ color: 'var(--text-secondary)' }}>
                <X size={18} />
              </button>
            </div>
            <div className="space-y-2 mb-4 text-xs" style={{ color: 'var(--text-secondary)' }}>
              <p>IP: <span className="font-mono" style={{ color: 'var(--text-primary)' }}>{labelModal.ip}</span></p>
              <p>MAC: <span className="font-mono" style={{ color: 'var(--text-primary)' }}>{labelModal.mac}</span></p>
              {labelModal.vendor && <p>Fabricante: <span style={{ color: 'var(--accent)' }}>{labelModal.vendor}</span></p>}
              {labelModal.hostname && <p>Hostname: <span style={{ color: 'var(--text-primary)' }}>{labelModal.hostname}</span></p>}
            </div>
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Nombre del dispositivo</label>
              <input
                type="text"
                value={labelInput}
                onChange={(e) => setLabelInput(e.target.value)}
                onKeyDown={(e) => e.key === 'Enter' && handleLabel()}
                placeholder="Ej: PC de Nick, Impresora oficina"
                className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                autoFocus
              />
            </div>
            <div className="flex items-center justify-end gap-3 mt-5">
              <button
                onClick={() => setLabelModal(null)}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
              >
                Cancelar
              </button>
              <button
                onClick={handleLabel}
                disabled={!labelInput.trim()}
                className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium"
                style={{ backgroundColor: 'var(--success)', color: '#ffffff' }}
              >
                <ShieldCheck size={14} />
                Marcar como conocido
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
