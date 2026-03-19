import { useEffect, useState, type ReactNode } from 'react'
import { HardDrive, Wifi, Activity, Database, Box } from 'lucide-react'
import { fetchDisks, fetchHosts, fetchHealth, fetchSystemInfo, fetchPrinters3D, fetchPrinter3DStatus } from '../api'
import type { DiskInfo, SystemInfo, NetworkHost, Printer3DConfig, Printer3DStatus } from '../types'

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}

interface StatCardProps {
  icon: ReactNode
  label: string
  value: string | number
}

function StatCard({ icon, label, value }: StatCardProps) {
  return (
    <div
      className="rounded-xl p-6 transition-all duration-200 hover:shadow-lg hover:-translate-y-0.5"
      style={{
        backgroundColor: 'var(--card-bg)',
        border: '1px solid var(--card-border)',
      }}
    >
      <div className="flex items-center gap-4">
        <div
          className="p-3 rounded-lg"
          style={{ backgroundColor: 'var(--accent-alpha)' }}
        >
          {icon}
        </div>
        <div>
          <p className="text-sm font-medium" style={{ color: 'var(--text-secondary)' }}>
            {label}
          </p>
          <p className="text-2xl font-bold mt-1" style={{ color: 'var(--text-primary)' }}>
            {value}
          </p>
        </div>
      </div>
    </div>
  )
}

export default function DashboardPage() {
  const [disks, setDisks] = useState<DiskInfo[]>([])
  const [systemInfo, setSystemInfo] = useState<SystemInfo | null>(null)
  const [hosts, setHosts] = useState<NetworkHost[]>([])
  const [_health, setHealth] = useState<any>(null)
  const [printers3d, setPrinters3d] = useState<Printer3DConfig[]>([])
  const [printerStatuses, setPrinterStatuses] = useState<Printer3DStatus[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    async function loadData() {
      setLoading(true)
      try {
        const [disksData, hostsData, healthData, sysInfoData, printers3dData] = await Promise.allSettled([
          fetchDisks(),
          fetchHosts(),
          fetchHealth(),
          fetchSystemInfo(),
          fetchPrinters3D(),
        ])
        if (disksData.status === 'fulfilled') setDisks(disksData.value)
        if (hostsData.status === 'fulfilled') setHosts(hostsData.value)
        if (healthData.status === 'fulfilled') setHealth(healthData.value)
        if (sysInfoData.status === 'fulfilled') setSystemInfo(sysInfoData.value)
        if (printers3dData.status === 'fulfilled') {
          setPrinters3d(printers3dData.value)
          // Fetch statuses
          const statusResults = await Promise.allSettled(
            printers3dData.value.map((p) => fetchPrinter3DStatus(p.id))
          )
          setPrinterStatuses(
            statusResults
              .filter((r): r is PromiseFulfilledResult<Printer3DStatus> => r.status === 'fulfilled')
              .map((r) => r.value)
          )
        }
      } finally {
        setLoading(false)
      }
    }
    loadData()
  }, [])

  const activeHosts = hosts.filter((h) => h.is_alive).length
  const totalSpace = disks.reduce((acc, d) => acc + d.total_space, 0)
  const availableSpace = disks.reduce((acc, d) => acc + d.available_space, 0)

  return (
    <div className="space-y-8">
      {/* Stats Row */}
      <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
        <StatCard
          icon={<HardDrive size={24} style={{ color: 'var(--accent)' }} />}
          label="Discos Montados"
          value={loading ? '...' : disks.length}
        />
        <StatCard
          icon={<Database size={24} style={{ color: 'var(--accent)' }} />}
          label="Espacio Total"
          value={loading ? '...' : formatBytes(totalSpace)}
        />
        <StatCard
          icon={<HardDrive size={24} style={{ color: 'var(--accent)' }} />}
          label="Espacio Disponible"
          value={loading ? '...' : formatBytes(availableSpace)}
        />
      </div>

      {/* Printers 3D Row */}
      {printers3d.length > 0 && (
        <div className="grid grid-cols-1 md:grid-cols-3 gap-6">
          <StatCard
            icon={<Box size={24} style={{ color: 'var(--accent)' }} />}
            label="Impresoras 3D"
            value={loading ? '...' : printers3d.length}
          />
          <StatCard
            icon={<Box size={24} style={{ color: 'var(--success)' }} />}
            label="En Linea"
            value={loading ? '...' : printerStatuses.filter((s) => s.online).length}
          />
          <StatCard
            icon={<Box size={24} style={{ color: 'var(--accent)' }} />}
            label="Imprimiendo"
            value={loading ? '...' : printerStatuses.filter((s) => s.current_job).length}
          />
        </div>
      )}

      {/* Second Row */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* Network Devices */}
        <div
          className="rounded-xl p-6 transition-all duration-200 hover:shadow-lg"
          style={{
            backgroundColor: 'var(--card-bg)',
            border: '1px solid var(--card-border)',
          }}
        >
          <div className="flex items-center gap-3 mb-4">
            <Wifi size={22} style={{ color: 'var(--accent)' }} />
            <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
              Dispositivos en la Red
            </h2>
          </div>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                Hosts activos
              </span>
              <span className="text-xl font-bold" style={{ color: 'var(--success)' }}>
                {loading ? '...' : activeHosts}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                Total detectados
              </span>
              <span className="text-xl font-bold" style={{ color: 'var(--text-primary)' }}>
                {loading ? '...' : hosts.length}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                Inactivos
              </span>
              <span className="text-xl font-bold" style={{ color: 'var(--danger)' }}>
                {loading ? '...' : hosts.length - activeHosts}
              </span>
            </div>
          </div>
        </div>

        {/* System Status */}
        <div
          className="rounded-xl p-6 transition-all duration-200 hover:shadow-lg"
          style={{
            backgroundColor: 'var(--card-bg)',
            border: '1px solid var(--card-border)',
          }}
        >
          <div className="flex items-center gap-3 mb-4">
            <Activity size={22} style={{ color: 'var(--accent)' }} />
            <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
              Estado del Sistema
            </h2>
          </div>
          <div className="space-y-3">
            <div className="flex items-center justify-between">
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                Hostname
              </span>
              <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                {loading ? '...' : systemInfo?.hostname ?? '--'}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                IP Local
              </span>
              <span className="text-sm font-medium font-mono" style={{ color: 'var(--accent)' }}>
                {loading ? '...' : systemInfo?.local_ip ?? '--'}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                SO
              </span>
              <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                {loading ? '...' : systemInfo?.os ?? '--'}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                Kernel
              </span>
              <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                {loading ? '...' : systemInfo?.kernel ?? '--'}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                RAM
              </span>
              <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                {loading ? '...' : systemInfo
                  ? `${formatBytes(systemInfo.used_memory)} / ${formatBytes(systemInfo.total_memory)}`
                  : '--'}
              </span>
            </div>
            <div className="flex items-center justify-between">
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                CPUs
              </span>
              <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                {loading ? '...' : systemInfo?.cpu_count ?? '--'}
              </span>
            </div>
          </div>
        </div>
      </div>
    </div>
  )
}
