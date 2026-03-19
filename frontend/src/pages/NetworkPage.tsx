import { useEffect, useState } from 'react'
import { useNavigate } from 'react-router-dom'
import { Radar, Monitor, Wifi, WifiOff, Loader2, Box } from 'lucide-react'
import { scanNetwork, fetchHosts } from '../api'
import type { NetworkHost } from '../types'

export default function NetworkPage() {
  const navigate = useNavigate()
  const [hosts, setHosts] = useState<NetworkHost[]>([])
  const [loading, setLoading] = useState(true)
  const [scanning, setScanning] = useState(false)

  useEffect(() => {
    loadHosts()
  }, [])

  async function loadHosts() {
    setLoading(true)
    try {
      const data = await fetchHosts()
      setHosts(data)
    } catch (err) {
      console.error('Error cargando hosts:', err)
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
    } catch (err) {
      console.error('Error escaneando red:', err)
    } finally {
      setScanning(false)
    }
  }

  const activeHosts = hosts.filter((h) => h.is_alive).length
  const inactiveHosts = hosts.length - activeHosts

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
          <div className="flex items-center gap-2">
            <WifiOff size={18} style={{ color: 'var(--danger)' }} />
            <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
              Inactivos: <strong style={{ color: 'var(--danger)' }}>{inactiveHosts}</strong>
            </span>
          </div>
        </div>

        <button
          onClick={handleScan}
          disabled={scanning}
          className="flex items-center gap-2 px-5 py-2.5 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
          style={{
            backgroundColor: 'var(--accent)',
            color: '#ffffff',
          }}
        >
          {scanning ? (
            <Loader2 size={18} className="animate-spin" />
          ) : (
            <Radar size={18} />
          )}
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
                <th
                  className="text-left px-6 py-3 text-xs font-medium uppercase tracking-wider"
                  style={{ color: 'var(--text-secondary)' }}
                >
                  IP
                </th>
                <th
                  className="text-left px-6 py-3 text-xs font-medium uppercase tracking-wider"
                  style={{ color: 'var(--text-secondary)' }}
                >
                  Hostname
                </th>
                <th
                  className="text-left px-6 py-3 text-xs font-medium uppercase tracking-wider"
                  style={{ color: 'var(--text-secondary)' }}
                >
                  Estado
                </th>
                <th
                  className="text-left px-6 py-3 text-xs font-medium uppercase tracking-wider"
                  style={{ color: 'var(--text-secondary)' }}
                >
                  Tiempo de Respuesta
                </th>
                <th
                  className="text-left px-6 py-3 text-xs font-medium uppercase tracking-wider"
                  style={{ color: 'var(--text-secondary)' }}
                >
                  Ultima vez visto
                </th>
                <th
                  className="text-right px-6 py-3 text-xs font-medium uppercase tracking-wider"
                  style={{ color: 'var(--text-secondary)' }}
                >
                  Acciones
                </th>
              </tr>
            </thead>
            <tbody>
              {hosts.map((host) => (
                <tr
                  key={host.ip}
                  className="transition-all duration-200 hover:opacity-90"
                  style={{ borderBottom: '1px solid var(--border)' }}
                >
                  <td className="px-6 py-3">
                    <span className="text-sm font-mono font-medium" style={{ color: 'var(--text-primary)' }}>
                      {host.ip}
                    </span>
                  </td>
                  <td className="px-6 py-3">
                    <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                      {host.hostname || '--'}
                    </span>
                  </td>
                  <td className="px-6 py-3">
                    <span
                      className="inline-flex items-center gap-1.5 px-2.5 py-0.5 rounded-full text-xs font-medium"
                      style={{
                        backgroundColor: host.is_alive ? 'var(--success-alpha)' : 'var(--danger-alpha)',
                        color: host.is_alive ? 'var(--success)' : 'var(--danger)',
                      }}
                    >
                      <span
                        className="w-1.5 h-1.5 rounded-full"
                        style={{ backgroundColor: host.is_alive ? 'var(--success)' : 'var(--danger)' }}
                      />
                      {host.is_alive ? 'Activo' : 'Inactivo'}
                    </span>
                  </td>
                  <td className="px-6 py-3">
                    <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                      {host.response_time_ms != null ? `${host.response_time_ms} ms` : '--'}
                    </span>
                  </td>
                  <td className="px-6 py-3">
                    <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                      {host.last_seen
                        ? new Date(host.last_seen).toLocaleString('es-ES')
                        : '--'}
                    </span>
                  </td>
                  <td className="px-6 py-3 text-right">
                    {host.is_alive && (
                      <button
                        onClick={() => navigate(`/printers3d?ip=${host.ip}`)}
                        className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-80"
                        style={{ color: 'var(--accent)', border: '1px solid var(--accent)' }}
                        title="Configurar como impresora 3D"
                      >
                        <Box size={14} />
                        3D
                      </button>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  )
}
