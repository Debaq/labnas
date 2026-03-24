import { useEffect, useState, useMemo, useRef } from 'react'
import type { ReactNode } from 'react'
import { HardDrive, Wifi, Activity, Database, Box, Music, Search, Play, Pause, Square, Loader2, X, SkipForward, SkipBack, Trash2, ListMusic, Plus, Sparkles, Speaker, Monitor, ExternalLink } from 'lucide-react'
import { fetchDisks, fetchHosts, fetchHealth, fetchSystemInfo, fetchPrinters3D, fetchPrinter3DStatus, searchMusic, playMusic, getCurrentMusic, stopMusic, pauseMusic, previousMusic, nextMusic, removeFromQueue, recommendMusic, setMusicMode, getServices, type MusicTrack, type MusicState, type LabService } from '../api'
import { useAuth } from '../auth/AuthContext'
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

  // Music
  const [musicState, setMusicState] = useState<MusicState>({ current: null, queue: [], started_by: null, history: [], mode: 'nas', stream_url: null, paused: false })
  const audioRef = useRef<HTMLAudioElement>(null)
  const [showSearch, setShowSearch] = useState(false)
  const [searchQuery, setSearchQuery] = useState('')
  const [searchResults, setSearchResults] = useState<MusicTrack[]>([])
  const [searching, setSearching] = useState(false)
  const [loadingTrack, setLoadingTrack] = useState(false)
  const [loadingMix, setLoadingMix] = useState(false)

  async function loadData(initial = false) {
    if (initial) setLoading(true)
    try {
      const [disksData, hostsData, healthData, sysInfoData, printers3dData, servicesData] = await Promise.allSettled([
        fetchDisks(),
        fetchHosts(),
        fetchHealth(),
        fetchSystemInfo(),
        fetchPrinters3D(),
        getServices(),
      ])
      if (disksData.status === 'fulfilled') setDisks(disksData.value)
      if (hostsData.status === 'fulfilled') setHosts(hostsData.value)
      if (healthData.status === 'fulfilled') setHealth(healthData.value)
      if (sysInfoData.status === 'fulfilled') setSystemInfo(sysInfoData.value)
      if (servicesData.status === 'fulfilled') setServices(servicesData.value)
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

  function safeMusicState(ms: MusicState): MusicState {
    return { current: ms.current ?? null, queue: ms.queue ?? [], started_by: ms.started_by ?? null, history: ms.history ?? [], mode: ms.mode ?? 'nas', stream_url: ms.stream_url ?? null, paused: ms.paused ?? false }
  }

  // Poll music state every 5s
  useEffect(() => {
    getCurrentMusic().then(ms => setMusicState(safeMusicState(ms))).catch(() => {})
    const interval = setInterval(() => {
      getCurrentMusic().then(ms => setMusicState(safeMusicState(ms))).catch(() => {})
    }, 5000)
    return () => clearInterval(interval)
  }, [])

  // Browser mode: sync audio + auto-next on ended
  useEffect(() => {
    const audio = audioRef.current
    if (!audio) return
    if (musicState.mode === 'browser' && musicState.stream_url) {
      if (audio.src !== musicState.stream_url) {
        audio.src = musicState.stream_url
        audio.play().catch(() => {})
      }
    } else if (musicState.mode === 'nas' || !musicState.stream_url) {
      audio.pause()
      audio.src = ''
    }
  }, [musicState.stream_url, musicState.mode])

  useEffect(() => {
    const audio = audioRef.current
    if (!audio) return
    const onEnded = () => {
      if (musicState.mode === 'browser') {
        nextMusic().then(ms => setMusicState(safeMusicState(ms))).catch(() => {})
      }
    }
    audio.addEventListener('ended', onEnded)
    return () => audio.removeEventListener('ended', onEnded)
  }, [musicState.mode])

  async function handleToggleMode() {
    const newMode = musicState.mode === 'nas' ? 'browser' : 'nas'
    setMusicState(safeMusicState(await setMusicMode(newMode)))
  }

  async function handleSearch() {
    if (!searchQuery.trim()) return
    setSearching(true)
    try {
      setSearchResults(await searchMusic(searchQuery))
    } catch {} finally {
      setSearching(false)
    }
  }

  async function handlePlay(id: string) {
    setLoadingTrack(true)
    try {
      setMusicState(safeMusicState(await playMusic(id)))
    } catch {} finally {
      setLoadingTrack(false)
    }
  }

  async function handleStop() {
    setMusicState(safeMusicState(await stopMusic()))
  }

  async function handlePause() {
    const ms = safeMusicState(await pauseMusic())
    setMusicState(ms)
    const audio = audioRef.current
    if (audio && musicState.mode === 'browser') {
      if (ms.paused) audio.pause()
      else audio.play().catch(() => {})
    }
  }

  async function handlePrevious() {
    try {
      setMusicState(safeMusicState(await previousMusic()))
    } catch {}
  }

  async function handleNext() {
    setMusicState(safeMusicState(await nextMusic()))
  }

  async function handleRemoveFromQueue(index: number) {
    setMusicState(safeMusicState(await removeFromQueue(index)))
  }

  async function handleRecommend() {
    setLoadingMix(true)
    try {
      setMusicState(safeMusicState(await recommendMusic()))
    } catch {} finally {
      setLoadingMix(false)
    }
  }

  function formatDuration(secs: number) {
    const m = Math.floor(secs / 60)
    const s = secs % 60
    return `${m}:${String(s).padStart(2, '0')}`
  }

  const activeHosts = hosts.filter((h) => h.is_alive).length
  const totalSpace = disks.reduce((acc, d) => acc + d.total_space, 0)
  const availableSpace = disks.reduce((acc, d) => acc + d.available_space, 0)

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

      {/* Music Player */}
      <div
        className="rounded-xl p-6 transition-all duration-200 hover:shadow-lg"
        style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
      >
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center gap-3">
            <Music size={22} style={{ color: 'var(--accent)' }} />
            <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Musica</h2>
            {musicState.queue.length > 0 && (
              <span className="text-xs px-2 py-0.5 rounded-full font-medium" style={{ backgroundColor: 'var(--accent)' + '25', color: 'var(--accent)' }}>
                {musicState.queue.length} en cola
              </span>
            )}
          </div>
          <div className="flex items-center gap-2">
            <button
              onClick={handleToggleMode}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all hover:opacity-90"
              style={{
                backgroundColor: 'var(--bg-tertiary)',
                color: 'var(--text-primary)',
                border: '1px solid var(--border)',
              }}
              title={musicState.mode === 'nas' ? 'Sonando en NAS' : 'Sonando en navegador'}
            >
              {musicState.mode === 'nas' ? <Speaker size={14} /> : <Monitor size={14} />}
              {musicState.mode === 'nas' ? 'NAS' : 'PC'}
            </button>
            <button
              onClick={handleRecommend}
              disabled={loadingMix || (!musicState.current && (musicState.history?.length ?? 0) === 0)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all hover:opacity-90"
              style={{
                backgroundColor: loadingMix ? 'var(--bg-tertiary)' : 'var(--success)' + '20',
                color: 'var(--success)',
                border: '1px solid var(--success)',
              }}
              title="Llenar cola con recomendaciones basadas en lo que suena"
            >
              {loadingMix ? <Loader2 size={14} className="animate-spin" /> : <Sparkles size={14} />}
              Mix
            </button>
            <button
              onClick={() => setShowSearch(!showSearch)}
              className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all hover:opacity-90"
              style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
            >
              <Search size={14} />
              Buscar
            </button>
          </div>
        </div>

        {/* Now Playing */}
        {musicState.current ? (
          <div className="flex items-center gap-4">
            <img
              src={musicState.current.thumbnail}
              alt=""
              className="w-16 h-16 rounded-lg object-cover shrink-0"
              style={{ border: '1px solid var(--border)' }}
            />
            <div className="flex-1 min-w-0">
              <p className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>
                {musicState.current.title}
              </p>
              <p className="text-xs truncate" style={{ color: 'var(--text-secondary)' }}>
                {musicState.current.artist}
              </p>
              <p className="text-[10px] mt-1" style={{ color: 'var(--text-secondary)', opacity: 0.6 }}>
                Puesto por {musicState.current.added_by || musicState.started_by} · {formatDuration(musicState.current.duration)}
              </p>
            </div>
            <div className="flex items-center gap-1.5">
              {musicState.history.length >= 2 && (
                <button onClick={handlePrevious}
                  className="p-2 rounded-lg transition-all hover:opacity-80"
                  style={{ backgroundColor: 'var(--accent)' + '20', color: 'var(--accent)' }}
                  title="Anterior">
                  <SkipBack size={16} />
                </button>
              )}
              <button onClick={handlePause}
                className="p-2 rounded-lg transition-all hover:opacity-80"
                style={{ backgroundColor: 'var(--accent)' + '20', color: 'var(--accent)' }}
                title={musicState.paused ? 'Reanudar' : 'Pausar'}>
                {musicState.paused ? <Play size={16} /> : <Pause size={16} />}
              </button>
              {musicState.queue.length > 0 && (
                <button onClick={handleNext}
                  className="p-2 rounded-lg transition-all hover:opacity-80"
                  style={{ backgroundColor: 'var(--accent)' + '20', color: 'var(--accent)' }}
                  title="Siguiente">
                  <SkipForward size={16} />
                </button>
              )}
              <button onClick={handleStop}
                className="p-2 rounded-lg transition-all hover:opacity-80"
                style={{ backgroundColor: 'var(--danger)' + '20', color: 'var(--danger)' }}
                title="Detener">
                <Square size={16} />
              </button>
            </div>
          </div>
        ) : (
          <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
            No hay musica reproduciendose
          </p>
        )}

        <audio ref={audioRef} style={{ display: 'none' }} />

        {/* Queue */}
        {musicState.queue.length > 0 && (
          <div className="mt-4 pt-3" style={{ borderTop: '1px solid var(--border)' }}>
            <div className="flex items-center gap-2 mb-2">
              <ListMusic size={14} style={{ color: 'var(--text-secondary)' }} />
              <span className="text-xs font-medium" style={{ color: 'var(--text-secondary)' }}>Cola de reproduccion</span>
            </div>
            <div className="space-y-1">
              {musicState.queue.map((track, i) => (
                <div key={`${track.id}-${i}`} className="flex items-center gap-3 px-3 py-2 rounded-lg"
                  style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                  <span className="text-xs font-mono w-5 text-center" style={{ color: 'var(--text-secondary)' }}>{i + 1}</span>
                  <img src={track.thumbnail} alt="" className="w-8 h-8 rounded object-cover shrink-0" />
                  <div className="flex-1 min-w-0">
                    <p className="text-xs truncate" style={{ color: 'var(--text-primary)' }}>{track.title}</p>
                    <p className="text-[10px] truncate" style={{ color: 'var(--text-secondary)' }}>
                      {track.artist} · {formatDuration(track.duration)}{track.added_by ? ` · ${track.added_by}` : ''}
                    </p>
                  </div>
                  <button onClick={() => handleRemoveFromQueue(i)}
                    className="p-1 rounded transition-all hover:opacity-80" style={{ color: 'var(--danger)' }}>
                    <Trash2 size={12} />
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Search */}
        {showSearch && (
          <div className="mt-4 pt-3" style={{ borderTop: '1px solid var(--border)' }}>
            <div className="flex items-center gap-2 mb-3">
              <input
                value={searchQuery}
                onChange={e => setSearchQuery(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleSearch()}
                placeholder="Buscar cancion o artista..."
                className="flex-1 px-3 py-2 rounded-lg text-sm outline-none"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                autoFocus
              />
              <button onClick={handleSearch} disabled={searching || !searchQuery.trim()}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                {searching ? <Loader2 size={16} className="animate-spin" /> : 'Buscar'}
              </button>
              <button onClick={() => { setShowSearch(false); setSearchResults([]) }}
                style={{ color: 'var(--text-secondary)' }}>
                <X size={18} />
              </button>
            </div>

            {loadingTrack && (
              <div className="flex items-center justify-center py-3 gap-2">
                <Loader2 size={16} className="animate-spin" style={{ color: 'var(--accent)' }} />
                <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Cargando...</span>
              </div>
            )}

            {searchResults.length > 0 && (
              <div className="space-y-1 max-h-72 overflow-y-auto">
                {searchResults.map(track => (
                  <div key={track.id}
                    className="flex items-center gap-3 px-3 py-2 rounded-lg cursor-pointer transition-all hover:opacity-80"
                    style={{ backgroundColor: 'var(--bg-tertiary)' }}
                    onClick={() => !loadingTrack && handlePlay(track.id)}>
                    <img src={track.thumbnail} alt="" className="w-10 h-10 rounded object-cover shrink-0" />
                    <div className="flex-1 min-w-0">
                      <p className="text-sm truncate" style={{ color: 'var(--text-primary)' }}>{track.title}</p>
                      <p className="text-xs truncate" style={{ color: 'var(--text-secondary)' }}>{track.artist} · {formatDuration(track.duration)}</p>
                    </div>
                    {musicState.current
                      ? <Plus size={16} style={{ color: 'var(--accent)' }} />
                      : <Play size={16} style={{ color: 'var(--accent)' }} />
                    }
                  </div>
                ))}
              </div>
            )}
          </div>
        )}
      </div>
    </div>
  )
}
