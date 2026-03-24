import { useEffect, useState, useRef } from 'react'
import {
  Music, Search, Play, Pause, Square, Loader2, X, SkipForward, SkipBack,
  Trash2, ListMusic, Plus, Sparkles, Speaker, Monitor, MoreVertical, Volume2,
  Shuffle, Repeat, Repeat1, ChevronUp, ChevronDown, ChevronRight,
} from 'lucide-react'
import {
  searchMusic, playMusic, getCurrentMusic, stopMusic, pauseMusic, previousMusic,
  nextMusic, removeFromQueue, playFromQueue, moveInQueue, toggleShuffle, toggleRepeat,
  recommendMusic, setMusicMode, setMusicVolume,
  type MusicTrack, type MusicState,
} from '../api'

function safeMusicState(ms: MusicState): MusicState {
  return {
    current: ms.current ?? null, queue: ms.queue ?? [], started_by: ms.started_by ?? null,
    history: ms.history ?? [], mode: ms.mode ?? 'nas', stream_url: ms.stream_url ?? null,
    paused: ms.paused ?? false, volume: ms.volume ?? 80, repeat: ms.repeat ?? 'off', shuffle: ms.shuffle ?? false,
  }
}

function formatDuration(secs: number) {
  const m = Math.floor(secs / 60)
  const s = secs % 60
  return `${m}:${String(s).padStart(2, '0')}`
}

export default function MusicPanel() {
  const [open, setOpen] = useState(() => localStorage.getItem('labnas-music-panel') !== 'closed')
  const [musicState, setMusicState] = useState<MusicState>({
    current: null, queue: [], started_by: null, history: [], mode: 'nas',
    stream_url: null, paused: false, volume: 80, repeat: 'off', shuffle: false,
  })
  const [showMenu, setShowMenu] = useState(false)
  const audioRef = useRef<HTMLAudioElement>(null)
  const [searchQuery, setSearchQuery] = useState('')
  const [searchResults, setSearchResults] = useState<MusicTrack[]>([])
  const [searching, setSearching] = useState(false)
  const [loadingTrack, setLoadingTrack] = useState(false)
  const [loadingMix, setLoadingMix] = useState(false)

  useEffect(() => {
    localStorage.setItem('labnas-music-panel', open ? 'open' : 'closed')
  }, [open])

  // Poll music state
  useEffect(() => {
    getCurrentMusic().then(ms => setMusicState(safeMusicState(ms))).catch(() => {})
    const interval = setInterval(() => {
      getCurrentMusic().then(ms => setMusicState(safeMusicState(ms))).catch(() => {})
    }, 5000)
    return () => clearInterval(interval)
  }, [])

  // Browser mode: sync audio
  useEffect(() => {
    const audio = audioRef.current
    if (!audio) return
    audio.volume = musicState.volume / 100
    if (musicState.mode === 'browser' && musicState.stream_url) {
      if (audio.src !== musicState.stream_url) {
        audio.src = musicState.stream_url
        audio.play().catch(() => {})
      }
    } else if (musicState.mode === 'nas' || !musicState.stream_url) {
      audio.pause()
      audio.src = ''
    }
  }, [musicState.stream_url, musicState.mode, musicState.volume])

  // Auto-next on ended/error
  useEffect(() => {
    const audio = audioRef.current
    if (!audio) return
    const autoNext = () => {
      if (musicState.mode === 'browser') {
        nextMusic().then(ms => setMusicState(safeMusicState(ms))).catch(() => {})
      }
    }
    audio.addEventListener('ended', autoNext)
    audio.addEventListener('error', autoNext)
    return () => {
      audio.removeEventListener('ended', autoNext)
      audio.removeEventListener('error', autoNext)
    }
  }, [musicState.mode])

  async function handleSearch() {
    if (!searchQuery.trim()) return
    setSearching(true)
    try { setSearchResults(await searchMusic(searchQuery)) } catch {} finally { setSearching(false) }
  }

  async function handlePlay(id: string) {
    setLoadingTrack(true)
    try { setMusicState(safeMusicState(await playMusic(id))) } catch {} finally { setLoadingTrack(false) }
  }

  async function handlePause() {
    const ms = safeMusicState(await pauseMusic())
    setMusicState(ms)
    const audio = audioRef.current
    if (audio && musicState.mode === 'browser') {
      if (ms.paused) audio.pause(); else audio.play().catch(() => {})
    }
  }

  async function handlePrevious() { try { setMusicState(safeMusicState(await previousMusic())) } catch {} }
  async function handleNext() { setMusicState(safeMusicState(await nextMusic())) }
  async function handleStop() { setMusicState(safeMusicState(await stopMusic())) }
  async function handleShuffle() { setMusicState(safeMusicState(await toggleShuffle())) }
  async function handleRepeat() { setMusicState(safeMusicState(await toggleRepeat())) }
  async function handlePlayFromQueue(i: number) { setMusicState(safeMusicState(await playFromQueue(i))) }
  async function handleMoveInQueue(from: number, to: number) { setMusicState(safeMusicState(await moveInQueue(from, to))) }
  async function handleRemoveFromQueue(i: number) { setMusicState(safeMusicState(await removeFromQueue(i))) }
  async function handleVolume(vol: number) {
    const ms = safeMusicState(await setMusicVolume(vol))
    setMusicState(ms)
    const audio = audioRef.current
    if (audio) audio.volume = vol / 100
  }
  async function handleToggleMode() {
    const newMode = musicState.mode === 'nas' ? 'browser' : 'nas'
    setMusicState(safeMusicState(await setMusicMode(newMode)))
  }
  async function handleRecommend() {
    setLoadingMix(true)
    try { setMusicState(safeMusicState(await recommendMusic())) } catch {} finally { setLoadingMix(false) }
  }

  // Mini floating button when closed
  if (!open) {
    return (
      <button
        onClick={() => setOpen(true)}
        className="fixed right-4 bottom-4 z-50 p-3 rounded-full shadow-lg transition-all hover:scale-105"
        style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
        title="Abrir reproductor"
      >
        <Music size={20} />
        {musicState.current && (
          <span className="absolute -top-1 -right-1 w-3 h-3 rounded-full animate-pulse" style={{ backgroundColor: 'var(--success)' }} />
        )}
      </button>
    )
  }

  return (
    <div
      className="flex flex-col h-screen transition-all duration-300"
      style={{
        width: '320px',
        minWidth: '320px',
        backgroundColor: 'var(--sidebar-bg)',
        borderLeft: '1px solid var(--border)',
      }}
    >
      {/* Header */}
      <div className="flex items-center justify-between px-4 py-3" style={{ borderBottom: '1px solid var(--border)' }}>
        <div className="flex items-center gap-2">
          <Music size={18} style={{ color: 'var(--accent)' }} />
          <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Musica</span>
          {musicState.queue.length > 0 && (
            <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium" style={{ backgroundColor: 'var(--accent)' + '25', color: 'var(--accent)' }}>
              {musicState.queue.length}
            </span>
          )}
        </div>
        <button onClick={() => setOpen(false)} className="p-1 rounded-lg hover:opacity-80" style={{ color: 'var(--text-secondary)' }}>
          <ChevronRight size={16} />
        </button>
      </div>

      {/* Search - always visible */}
      <div className="px-3 py-2" style={{ borderBottom: '1px solid var(--border)' }}>
        <div className="flex items-center gap-2">
          <input
            value={searchQuery}
            onChange={e => setSearchQuery(e.target.value)}
            onKeyDown={e => e.key === 'Enter' && handleSearch()}
            placeholder="Buscar cancion..."
            className="flex-1 px-3 py-1.5 rounded-lg text-xs outline-none"
            style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
          />
          <button onClick={handleSearch} disabled={searching || !searchQuery.trim()}
            className="p-1.5 rounded-lg" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            {searching ? <Loader2 size={14} className="animate-spin" /> : <Search size={14} />}
          </button>
        </div>
      </div>

      {/* Scrollable content */}
      <div className="flex-1 overflow-y-auto">
        {/* Search results */}
        {searchResults.length > 0 && (
          <div className="px-3 py-2" style={{ borderBottom: '1px solid var(--border)' }}>
            <div className="flex items-center justify-between mb-2">
              <span className="text-[10px] font-medium" style={{ color: 'var(--text-secondary)' }}>Resultados</span>
              <button onClick={() => setSearchResults([])} className="p-0.5" style={{ color: 'var(--text-secondary)' }}>
                <X size={12} />
              </button>
            </div>
            {loadingTrack && (
              <div className="flex items-center justify-center py-2 gap-1">
                <Loader2 size={12} className="animate-spin" style={{ color: 'var(--accent)' }} />
                <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>Cargando...</span>
              </div>
            )}
            <div className="space-y-1 max-h-48 overflow-y-auto">
              {searchResults.map(track => (
                <div key={track.id}
                  className="flex items-center gap-2 px-2 py-1.5 rounded-lg cursor-pointer transition-all hover:opacity-80"
                  style={{ backgroundColor: 'var(--bg-tertiary)' }}
                  onClick={() => !loadingTrack && handlePlay(track.id)}>
                  <img src={track.thumbnail} alt="" className="w-8 h-8 rounded object-cover shrink-0" />
                  <div className="flex-1 min-w-0">
                    <p className="text-[11px] truncate" style={{ color: 'var(--text-primary)' }}>{track.title}</p>
                    <p className="text-[10px] truncate" style={{ color: 'var(--text-secondary)' }}>{track.artist} · {formatDuration(track.duration)}</p>
                  </div>
                  {musicState.current ? <Plus size={14} style={{ color: 'var(--accent)' }} /> : <Play size={14} style={{ color: 'var(--accent)' }} />}
                </div>
              ))}
            </div>
          </div>
        )}

        {/* Now Playing */}
        <div className="px-3 py-3">
          {musicState.current ? (
            <div className="space-y-3">
              <div className="flex items-start gap-3">
                <img src={musicState.current.thumbnail} alt="" className="w-14 h-14 rounded-lg object-cover shrink-0" style={{ border: '1px solid var(--border)' }} />
                <div className="flex-1 min-w-0">
                  <p className="text-xs font-medium leading-tight" style={{ color: 'var(--text-primary)' }}>{musicState.current.title}</p>
                  <p className="text-[10px] mt-0.5" style={{ color: 'var(--text-secondary)' }}>{musicState.current.artist}</p>
                  <p className="text-[9px] mt-1" style={{ color: 'var(--text-secondary)', opacity: 0.6 }}>
                    {musicState.current.added_by || musicState.started_by} · {formatDuration(musicState.current.duration)}
                  </p>
                </div>
              </div>

              {/* Controls */}
              <div className="flex items-center justify-center gap-1">
                <button onClick={handleShuffle} className="p-1.5 rounded-lg hover:opacity-80"
                  style={{ color: musicState.shuffle ? 'var(--accent)' : 'var(--text-secondary)' }}>
                  <Shuffle size={13} />
                </button>
                {musicState.history.length >= 2 && (
                  <button onClick={handlePrevious} className="p-2 rounded-lg hover:opacity-80" style={{ color: 'var(--accent)' }}>
                    <SkipBack size={16} />
                  </button>
                )}
                <button onClick={handlePause}
                  className="p-2.5 rounded-full hover:opacity-80"
                  style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                  {musicState.paused ? <Play size={18} /> : <Pause size={18} />}
                </button>
                <button onClick={handleNext} className="p-2 rounded-lg hover:opacity-80" style={{ color: 'var(--accent)' }}>
                  <SkipForward size={16} />
                </button>
                <button onClick={handleRepeat} className="p-1.5 rounded-lg hover:opacity-80"
                  style={{ color: musicState.repeat !== 'off' ? 'var(--accent)' : 'var(--text-secondary)' }}>
                  {musicState.repeat === 'one' ? <Repeat1 size={13} /> : <Repeat size={13} />}
                </button>
              </div>

              {/* Volume + extras */}
              <div className="flex items-center gap-2">
                <Volume2 size={12} style={{ color: 'var(--text-secondary)' }} />
                <input type="range" min="0" max="100" value={musicState.volume}
                  onChange={e => handleVolume(parseInt(e.target.value))}
                  className="flex-1 h-1 rounded-full appearance-none cursor-pointer"
                  style={{ accentColor: 'var(--accent)', backgroundColor: 'var(--bg-tertiary)' }} />
                <span className="text-[9px] w-7 text-right" style={{ color: 'var(--text-secondary)' }}>{musicState.volume}%</span>
              </div>

              {/* Action buttons */}
              <div className="flex items-center gap-1.5">
                <button onClick={handleRecommend}
                  disabled={loadingMix}
                  className="flex-1 flex items-center justify-center gap-1 py-1.5 rounded-lg text-[10px] font-medium hover:opacity-90"
                  style={{ backgroundColor: 'var(--success)' + '20', color: 'var(--success)', border: '1px solid var(--success)' + '40' }}>
                  {loadingMix ? <Loader2 size={12} className="animate-spin" /> : <Sparkles size={12} />}
                  Mix
                </button>
                <button onClick={handleStop}
                  className="flex items-center justify-center gap-1 px-3 py-1.5 rounded-lg text-[10px] font-medium hover:opacity-90"
                  style={{ color: 'var(--danger)', border: '1px solid var(--danger)' + '40' }}>
                  <Square size={12} />
                </button>
                <div className="relative">
                  <button onClick={() => setShowMenu(!showMenu)} className="p-1.5 rounded-lg hover:opacity-80" style={{ color: 'var(--text-secondary)' }}>
                    <MoreVertical size={14} />
                  </button>
                  {showMenu && (
                    <>
                      <div className="fixed inset-0 z-10" onClick={() => setShowMenu(false)} />
                      <div className="absolute right-0 bottom-full mb-1 z-20 rounded-lg p-2 min-w-[160px]"
                        style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)', boxShadow: '0 4px 12px rgba(0,0,0,0.3)' }}>
                        <button onClick={() => { handleToggleMode(); setShowMenu(false) }}
                          className="flex items-center gap-2 w-full px-3 py-2 rounded-lg text-[11px] font-medium hover:opacity-80"
                          style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}>
                          {musicState.mode === 'nas' ? <Speaker size={12} /> : <Monitor size={12} />}
                          Modo: {musicState.mode === 'nas' ? 'NAS' : 'PC'}
                        </button>
                      </div>
                    </>
                  )}
                </div>
              </div>
            </div>
          ) : (
            <div className="text-center py-4">
              <Music size={32} className="mx-auto mb-2" style={{ color: 'var(--text-secondary)', opacity: 0.3 }} />
              <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Sin musica</p>
              {musicState.queue.length > 0 && (
                <button onClick={handleNext}
                  className="mt-2 flex items-center justify-center gap-1 mx-auto px-3 py-1.5 rounded-lg text-[10px] font-medium hover:opacity-90"
                  style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                  <Play size={12} />
                  Reproducir cola ({musicState.queue.length})
                </button>
              )}
            </div>
          )}
        </div>

        {/* Queue */}
        {musicState.queue.length > 0 && (
          <div className="px-3 pb-3">
            <div className="flex items-center gap-1.5 mb-2" style={{ borderTop: '1px solid var(--border)', paddingTop: '8px' }}>
              <ListMusic size={12} style={{ color: 'var(--text-secondary)' }} />
              <span className="text-[10px] font-medium" style={{ color: 'var(--text-secondary)' }}>Cola ({musicState.queue.length})</span>
            </div>
            <div className="space-y-1">
              {musicState.queue.map((track, i) => (
                <div key={`${track.id}-${i}`} className="flex items-center gap-1.5 px-2 py-1.5 rounded-lg group"
                  style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                  <span className="text-[9px] font-mono w-4 text-center shrink-0" style={{ color: 'var(--text-secondary)' }}>{i + 1}</span>
                  <img src={track.thumbnail} alt="" className="w-7 h-7 rounded object-cover shrink-0 cursor-pointer hover:opacity-80"
                    onClick={() => handlePlayFromQueue(i)} />
                  <div className="flex-1 min-w-0 cursor-pointer" onClick={() => handlePlayFromQueue(i)}>
                    <p className="text-[10px] truncate leading-tight" style={{ color: 'var(--text-primary)' }}>{track.title}</p>
                    <p className="text-[9px] truncate" style={{ color: 'var(--text-secondary)' }}>{track.artist} · {formatDuration(track.duration)}</p>
                  </div>
                  <div className="flex items-center gap-0 opacity-0 group-hover:opacity-100 transition-opacity">
                    {i > 0 && (
                      <button onClick={() => handleMoveInQueue(i, i - 1)} className="p-0.5" style={{ color: 'var(--text-secondary)' }}>
                        <ChevronUp size={10} />
                      </button>
                    )}
                    {i < musicState.queue.length - 1 && (
                      <button onClick={() => handleMoveInQueue(i, i + 1)} className="p-0.5" style={{ color: 'var(--text-secondary)' }}>
                        <ChevronDown size={10} />
                      </button>
                    )}
                  </div>
                  <button onClick={() => handleRemoveFromQueue(i)} className="p-0.5 rounded hover:opacity-80 opacity-0 group-hover:opacity-100 transition-opacity"
                    style={{ color: 'var(--danger)' }}>
                    <Trash2 size={10} />
                  </button>
                </div>
              ))}
            </div>
          </div>
        )}
      </div>

      <audio ref={audioRef} style={{ display: 'none' }} />
    </div>
  )
}
