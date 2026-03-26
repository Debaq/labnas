import { useEffect, useState, useMemo } from 'react'
import type { ReactNode } from 'react'
import { HardDrive, Wifi, Activity, Database, Box, ExternalLink, ClipboardList, Calendar, Clock, Users, FolderOpen } from 'lucide-react'
import { fetchDisks, fetchHosts, fetchHealth, fetchSystemInfo, fetchPrinters3D, fetchPrinter3DStatus, getServices, fetchTasks, fetchEvents, fetchProjects, type LabService } from '../api'
import { useAuth } from '../auth/AuthContext'
import type { DiskInfo, SystemInfo, NetworkHost, Printer3DConfig, Printer3DStatus, Task, CalendarEvent, Project } from '../types'

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

function getGreeting(): string {
  const hour = new Date().getHours()
  if (hour < 12) return 'Buenos dias'
  if (hour < 20) return 'Buenas tardes'
  return 'Buenas noches'
}

function getFormattedDate(): string {
  const now = new Date()
  const days = ['Domingo', 'Lunes', 'Martes', 'Miercoles', 'Jueves', 'Viernes', 'Sabado']
  const months = ['enero', 'febrero', 'marzo', 'abril', 'mayo', 'junio', 'julio', 'agosto', 'septiembre', 'octubre', 'noviembre', 'diciembre']
  return `${days[now.getDay()]}, ${now.getDate()} de ${months[now.getMonth()]} de ${now.getFullYear()}`
}

export default function DashboardPage() {
  const { user } = useAuth()
  const greeting = useMemo(() => getGreeting(), [])
  const formattedDate = useMemo(() => getFormattedDate(), [])

  const [disks, setDisks] = useState<DiskInfo[]>([])
  const [systemInfo, setSystemInfo] = useState<SystemInfo | null>(null)
  const [hosts, setHosts] = useState<NetworkHost[]>([])
  const [_health, setHealth] = useState<any>(null)
  const [printers3d, setPrinters3d] = useState<Printer3DConfig[]>([])
  const [printerStatuses, setPrinterStatuses] = useState<Printer3DStatus[]>([])
  const [loading, setLoading] = useState(true)
  const [services, setServices] = useState<LabService[]>([])
  const [tasks, setTasks] = useState<Task[]>([])
  const [events, setEvents] = useState<CalendarEvent[]>([])
  const [projects, setProjects] = useState<Project[]>([])

  async function loadData(initial = false) {
    if (initial) setLoading(true)
    try {
      const [disksData, hostsData, healthData, sysInfoData, printers3dData, servicesData, tasksData, eventsData, projectsData] = await Promise.allSettled([
        fetchDisks(),
        fetchHosts(),
        fetchHealth(),
        fetchSystemInfo(),
        fetchPrinters3D(),
        getServices(),
        fetchTasks(),
        fetchEvents(),
        fetchProjects(),
      ])
      if (disksData.status === 'fulfilled') setDisks(disksData.value)
      if (hostsData.status === 'fulfilled') setHosts(hostsData.value)
      if (healthData.status === 'fulfilled') setHealth(healthData.value)
      if (sysInfoData.status === 'fulfilled') setSystemInfo(sysInfoData.value)
      if (servicesData.status === 'fulfilled') setServices(servicesData.value)
      if (tasksData.status === 'fulfilled') setTasks(tasksData.value)
      if (eventsData.status === 'fulfilled') setEvents(eventsData.value)
      if (projectsData.status === 'fulfilled') setProjects(projectsData.value)
      if (printers3dData.status === 'fulfilled') {
        setPrinters3d(printers3dData.value)
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
      if (initial) setLoading(false)
    }
  }

  useEffect(() => {
    loadData(true)
    // Refrescar dashboard cada 30 segundos
    const interval = setInterval(() => loadData(), 30000)
    return () => clearInterval(interval)
  }, [])

  const activeHosts = hosts.filter((h) => h.is_alive).length
  const totalSpace = disks.reduce((acc, d) => acc + d.total_space, 0)
  const availableSpace = disks.reduce((acc, d) => acc + d.available_space, 0)

  // Tareas pendientes/en progreso, ordenadas: las con fecha primero
  const pendingTasks = tasks
    .filter((t) => t.status === 'pendiente' || t.status === 'enprogreso')
    .sort((a, b) => {
      if (a.due_date && !b.due_date) return -1
      if (!a.due_date && b.due_date) return 1
      if (a.due_date && b.due_date) return a.due_date.localeCompare(b.due_date)
      return 0
    })
    .slice(0, 5)

  // Eventos futuros, ordenados por fecha/hora
  const now = new Date().toISOString().slice(0, 16).replace('T', ' ')
  const upcomingEvents = events
    .filter((e) => `${e.date} ${e.time}` >= now)
    .sort((a, b) => `${a.date} ${a.time}`.localeCompare(`${b.date} ${b.time}`))
    .slice(0, 5)

  return (
    <div className="space-y-8">
      {/* Saludo */}
      <div>
        <h1 className="text-2xl font-bold" style={{ color: 'var(--text-primary)' }}>
          {greeting}, {user?.username}
        </h1>
        <p className="text-sm mt-1" style={{ color: 'var(--text-secondary)' }}>
          {formattedDate}
        </p>
      </div>

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

      {/* Tareas pendientes y Eventos proximos */}
      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        {/* Tareas pendientes */}
        <div
          className="rounded-xl p-6 transition-all duration-200 hover:shadow-lg"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <div className="flex items-center gap-3 mb-4">
            <ClipboardList size={22} style={{ color: 'var(--accent)' }} />
            <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
              Tareas Pendientes
            </h2>
            <span className="text-xs px-2 py-0.5 rounded-full" style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}>
              {tasks.filter((t) => t.status === 'pendiente' || t.status === 'enprogreso').length}
            </span>
          </div>
          {pendingTasks.length === 0 ? (
            <p className="text-sm py-4 text-center" style={{ color: 'var(--text-secondary)' }}>
              Sin tareas pendientes
            </p>
          ) : (
            <div className="space-y-2.5">
              {pendingTasks.map((task) => {
                const projectName = task.project_id
                  ? projects.find((p) => p.id === task.project_id)?.name
                  : null
                return (
                  <div
                    key={task.id}
                    className="flex items-start gap-3 p-3 rounded-lg"
                    style={{ backgroundColor: 'var(--bg-tertiary)' }}
                  >
                    <div
                      className="w-2 h-2 rounded-full mt-1.5 shrink-0"
                      style={{
                        backgroundColor: task.status === 'enprogreso' ? 'var(--accent)' : 'var(--warning)',
                      }}
                    />
                    <div className="flex-1 min-w-0">
                      <p className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>
                        {task.title}
                      </p>
                      <div className="flex items-center gap-3 mt-1 text-xs" style={{ color: 'var(--text-secondary)' }}>
                        {projectName && (
                          <span className="flex items-center gap-1">
                            <FolderOpen size={10} />
                            {projectName}
                          </span>
                        )}
                        {task.assigned_to.length > 0 && (
                          <span className="flex items-center gap-1">
                            <Users size={10} />
                            {task.assigned_to.join(', ')}
                          </span>
                        )}
                        {task.due_date && (
                          <span className="flex items-center gap-1">
                            <Clock size={10} />
                            {task.due_date}{task.due_time ? ` ${task.due_time}` : ''}
                          </span>
                        )}
                      </div>
                    </div>
                  </div>
                )
              })}
            </div>
          )}
        </div>

        {/* Proximos eventos */}
        <div
          className="rounded-xl p-6 transition-all duration-200 hover:shadow-lg"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <div className="flex items-center gap-3 mb-4">
            <Calendar size={22} style={{ color: 'var(--accent)' }} />
            <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
              Proximos Eventos
            </h2>
            <span className="text-xs px-2 py-0.5 rounded-full" style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}>
              {upcomingEvents.length}
            </span>
          </div>
          {upcomingEvents.length === 0 ? (
            <p className="text-sm py-4 text-center" style={{ color: 'var(--text-secondary)' }}>
              Sin eventos proximos
            </p>
          ) : (
            <div className="space-y-2.5">
              {upcomingEvents.map((ev) => (
                <div
                  key={ev.id}
                  className="flex items-start gap-3 p-3 rounded-lg"
                  style={{ backgroundColor: 'var(--bg-tertiary)' }}
                >
                  <div
                    className="p-1.5 rounded-lg shrink-0"
                    style={{ backgroundColor: 'var(--accent-alpha)' }}
                  >
                    <Calendar size={14} style={{ color: 'var(--accent)' }} />
                  </div>
                  <div className="flex-1 min-w-0">
                    <p className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>
                      {ev.title}
                    </p>
                    <div className="flex items-center gap-3 mt-1 text-xs" style={{ color: 'var(--text-secondary)' }}>
                      <span className="font-mono" style={{ color: 'var(--accent)' }}>
                        {ev.date} {ev.time}
                      </span>
                      {ev.invitees.length > 0 && (
                        <span className="flex items-center gap-1">
                          <Users size={10} />
                          {ev.invitees.join(', ')}
                        </span>
                      )}
                      {ev.recurrence && ev.recurrence !== 'none' && (
                        <span className="px-1.5 py-0.5 rounded text-[10px]" style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}>
                          {ev.recurrence === 'daily' ? 'Diario' : ev.recurrence === 'weekly' ? 'Semanal' : 'Mensual'}
                        </span>
                      )}
                    </div>
                  </div>
                </div>
              ))}
            </div>
          )}
        </div>
      </div>

      {/* Servicios del Lab */}
      {services.length > 0 && (
        <div
          className="rounded-xl p-6 transition-all duration-200 hover:shadow-lg"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <div className="flex items-center gap-3 mb-4">
            <ExternalLink size={22} style={{ color: 'var(--accent)' }} />
            <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
              Servicios del Lab
            </h2>
          </div>
          <div className="grid grid-cols-1 sm:grid-cols-2 md:grid-cols-3 gap-3">
            {services.map(svc => (
              <a
                key={svc.port}
                href={`${window.location.protocol}//${window.location.hostname}:${svc.port}`}
                target="_blank"
                rel="noopener noreferrer"
                className="flex items-center gap-3 p-4 rounded-lg transition-all hover:opacity-80"
                style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}
              >
                <div className="p-2 rounded-lg" style={{ backgroundColor: 'var(--accent-alpha)' }}>
                  <span className="text-lg">{svc.icon || '🔗'}</span>
                </div>
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>{svc.name}</p>
                  {svc.description && (
                    <p className="text-xs truncate" style={{ color: 'var(--text-secondary)' }}>{svc.description}</p>
                  )}
                  <p className="text-xs font-mono" style={{ color: 'var(--accent)' }}>:{svc.port}</p>
                </div>
                <ExternalLink size={14} style={{ color: 'var(--text-secondary)' }} />
              </a>
            ))}
          </div>
        </div>
      )}

    </div>
  )
}
