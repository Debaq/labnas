import { useEffect, useState, useRef } from 'react'
import { useParams, useNavigate } from 'react-router-dom'
import {
  ArrowLeft, Search, Plus, Trash2, ChevronUp, ChevronDown, Play, Loader2,
  ListMusic, Clock, Music, GripVertical,
} from 'lucide-react'
import {
  fetchPlaylists, updatePlaylist, addTrackToPlaylist, removeTrackFromPlaylist,
  moveTrackInPlaylist, loadPlaylist, searchMusic, getCurrentMusic, playMusic,
  type Playlist, type MusicTrack, type MusicState,
} from '../api'

function formatDuration(secs: number) {
  const m = Math.floor(secs / 60)
  const s = secs % 60
  return `${m}:${String(s).padStart(2, '0')}`
}

function formatTotalDuration(secs: number) {
  const h = Math.floor(secs / 3600)
  const m = Math.floor((secs % 3600) / 60)
  if (h > 0) return `${h}h ${m}m`
  return `${m} min`
}

export default function PlaylistEditorPage() {
  const { id } = useParams<{ id: string }>()
  const navigate = useNavigate()
  const [playlist, setPlaylist] = useState<Playlist | null>(null)
  const [loading, setLoading] = useState(true)
  const [editingName, setEditingName] = useState(false)
  const [editName, setEditName] = useState('')
  const [editingDesc, setEditingDesc] = useState(false)
  const [editDesc, setEditDesc] = useState('')
  const [searchQuery, setSearchQuery] = useState('')
  const [searchResults, setSearchResults] = useState<MusicTrack[]>([])
  const [searching, setSearching] = useState(false)
  const [loadingTrack, setLoadingTrack] = useState<string | null>(null)
  const [loadingQueue, setLoadingQueue] = useState(false)
  const searchRef = useRef<HTMLInputElement>(null)
  // Drag state
  const [dragIdx, setDragIdx] = useState<number | null>(null)
  const [dragOverIdx, setDragOverIdx] = useState<number | null>(null)

  useEffect(() => {
    loadData()
  }, [id])

  // Focus search on mount
  useEffect(() => {
    if (!loading && searchRef.current) searchRef.current.focus()
  }, [loading])

  async function loadData() {
    setLoading(true)
    try {
      const all = await fetchPlaylists()
      const found = all.find(p => p.id === id)
      if (found) {
        setPlaylist(found)
        setEditName(found.name)
        setEditDesc(found.description || '')
      }
    } catch {} finally { setLoading(false) }
  }

  async function handleSearch() {
    if (!searchQuery.trim()) return
    setSearching(true)
    try { setSearchResults(await searchMusic(searchQuery)) } catch {} finally { setSearching(false) }
  }

  async function handleAddTrack(track: MusicTrack) {
    if (!playlist) return
    setLoadingTrack(track.id)
    try {
      const updated = await addTrackToPlaylist(playlist.id, {
        id: track.id, title: track.title, artist: track.artist,
        thumbnail: track.thumbnail, duration: track.duration,
      })
      setPlaylist(updated)
    } catch {} finally { setLoadingTrack(null) }
  }

  async function handleRemoveTrack(index: number) {
    if (!playlist) return
    const updated = await removeTrackFromPlaylist(playlist.id, index)
    setPlaylist(updated)
  }

  async function handleMoveTrack(from: number, to: number) {
    if (!playlist || from === to) return
    const updated = await moveTrackInPlaylist(playlist.id, from, to)
    setPlaylist(updated)
  }

  async function handleSaveName() {
    if (!playlist || !editName.trim() || editName === playlist.name) { setEditingName(false); return }
    const updated = await updatePlaylist(playlist.id, { name: editName })
    setPlaylist(updated)
    setEditingName(false)
  }

  async function handleSaveDesc() {
    if (!playlist) { setEditingDesc(false); return }
    if (editDesc === (playlist.description || '')) { setEditingDesc(false); return }
    const updated = await updatePlaylist(playlist.id, { description: editDesc })
    setPlaylist(updated)
    setEditingDesc(false)
  }

  async function handleLoadToQueue() {
    if (!playlist) return
    setLoadingQueue(true)
    try { await loadPlaylist(playlist.id) } catch {} finally { setLoadingQueue(false) }
  }

  async function handlePlayTrack(trackId: string) {
    setLoadingTrack(trackId)
    try { await playMusic(trackId) } catch {} finally { setLoadingTrack(null) }
  }

  // Drag & drop handlers
  function handleDragStart(idx: number) { setDragIdx(idx) }
  function handleDragOver(e: React.DragEvent, idx: number) { e.preventDefault(); setDragOverIdx(idx) }
  async function handleDrop(idx: number) {
    if (dragIdx !== null && dragIdx !== idx) await handleMoveTrack(dragIdx, idx)
    setDragIdx(null); setDragOverIdx(null)
  }
  function handleDragEnd() { setDragIdx(null); setDragOverIdx(null) }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-full">
        <Loader2 size={32} className="animate-spin" style={{ color: 'var(--accent)' }} />
      </div>
    )
  }

  if (!playlist) {
    return (
      <div className="flex flex-col items-center justify-center h-full gap-4">
        <ListMusic size={48} style={{ color: 'var(--text-secondary)' }} />
        <p style={{ color: 'var(--text-secondary)' }}>Playlist no encontrada</p>
        <button onClick={() => navigate(-1)} className="px-4 py-2 rounded-lg text-sm font-medium"
          style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
          Volver
        </button>
      </div>
    )
  }

  const totalDuration = playlist.tracks.reduce((sum, t) => sum + t.duration, 0)

  return (
    <div className="h-full flex flex-col">
      {/* Header */}
      <div className="flex items-center gap-4 mb-6">
        <button onClick={() => navigate(-1)} className="p-2 rounded-lg hover:opacity-80"
          style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}>
          <ArrowLeft size={20} />
        </button>
        <div className="flex-1 min-w-0">
          {editingName ? (
            <input value={editName} onChange={e => setEditName(e.target.value)}
              onBlur={handleSaveName}
              onKeyDown={e => { if (e.key === 'Enter') handleSaveName(); if (e.key === 'Escape') setEditingName(false) }}
              autoFocus className="text-2xl font-bold w-full px-2 py-1 rounded-lg outline-none"
              style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--accent)' }} />
          ) : (
            <h1 className="text-2xl font-bold cursor-pointer hover:opacity-80 truncate"
              style={{ color: 'var(--text-primary)' }}
              onClick={() => { setEditingName(true); setEditName(playlist.name) }}>
              {playlist.name}
            </h1>
          )}
          {editingDesc ? (
            <input value={editDesc} onChange={e => setEditDesc(e.target.value)}
              onBlur={handleSaveDesc}
              onKeyDown={e => { if (e.key === 'Enter') handleSaveDesc(); if (e.key === 'Escape') setEditingDesc(false) }}
              autoFocus placeholder="Agregar descripcion..."
              className="text-sm w-full px-2 py-1 mt-1 rounded-lg outline-none"
              style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-secondary)', border: '1px solid var(--input-border)' }} />
          ) : (
            <p className="text-sm mt-0.5 cursor-pointer hover:opacity-80"
              style={{ color: 'var(--text-secondary)' }}
              onClick={() => { setEditingDesc(true); setEditDesc(playlist.description || '') }}>
              {playlist.description || 'Sin descripcion — clic para agregar'}
            </p>
          )}
        </div>
        <div className="flex items-center gap-3 shrink-0">
          <div className="text-right">
            <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>{playlist.tracks.length} tracks</p>
            <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
              <Clock size={10} className="inline mr-1" />{formatTotalDuration(totalDuration)}
            </p>
          </div>
          <button onClick={handleLoadToQueue} disabled={loadingQueue || playlist.tracks.length === 0}
            className="flex items-center gap-2 px-4 py-2.5 rounded-lg text-sm font-medium hover:opacity-90 transition-opacity"
            style={{ backgroundColor: 'var(--accent)', color: '#fff', opacity: playlist.tracks.length === 0 ? 0.4 : 1 }}>
            {loadingQueue ? <Loader2 size={16} className="animate-spin" /> : <Play size={16} />}
            Cargar en cola
          </button>
        </div>
      </div>

      {/* Two-column layout */}
      <div className="flex-1 flex gap-6 min-h-0">
        {/* Left: Track list */}
        <div className="flex-1 flex flex-col min-w-0 rounded-xl overflow-hidden"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
          <div className="flex items-center gap-2 px-4 py-3" style={{ borderBottom: '1px solid var(--border)' }}>
            <ListMusic size={16} style={{ color: 'var(--accent)' }} />
            <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Tracks de la lista</span>
            <span className="text-xs ml-auto" style={{ color: 'var(--text-secondary)' }}>Arrastra para reordenar</span>
          </div>
          <div className="flex-1 overflow-y-auto">
            {playlist.tracks.length === 0 ? (
              <div className="flex flex-col items-center justify-center h-full gap-3 py-12">
                <Music size={40} style={{ color: 'var(--text-secondary)', opacity: 0.3 }} />
                <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Lista vacia</p>
                <p className="text-xs" style={{ color: 'var(--text-secondary)', opacity: 0.6 }}>Busca canciones a la derecha para agregar</p>
              </div>
            ) : (
              <div className="divide-y" style={{ borderColor: 'var(--border)' }}>
                {playlist.tracks.map((track, i) => (
                  <div key={`${track.id}-${i}`}
                    draggable
                    onDragStart={() => handleDragStart(i)}
                    onDragOver={e => handleDragOver(e, i)}
                    onDrop={() => handleDrop(i)}
                    onDragEnd={handleDragEnd}
                    className="flex items-center gap-3 px-4 py-2.5 group transition-colors"
                    style={{
                      backgroundColor: dragOverIdx === i ? 'var(--accent)' + '15' : dragIdx === i ? 'var(--bg-tertiary)' : 'transparent',
                      opacity: dragIdx === i ? 0.5 : 1,
                    }}>
                    <GripVertical size={14} className="shrink-0 cursor-grab opacity-30 group-hover:opacity-100 transition-opacity"
                      style={{ color: 'var(--text-secondary)' }} />
                    <span className="text-xs font-mono w-6 text-center shrink-0" style={{ color: 'var(--text-secondary)' }}>{i + 1}</span>
                    <img src={track.thumbnail} alt="" className="w-10 h-10 rounded-lg object-cover shrink-0 cursor-pointer hover:opacity-80"
                      style={{ border: '1px solid var(--border)' }}
                      onClick={() => handlePlayTrack(track.id)} />
                    <div className="flex-1 min-w-0">
                      <p className="text-sm truncate" style={{ color: 'var(--text-primary)' }}>{track.title}</p>
                      <p className="text-xs truncate" style={{ color: 'var(--text-secondary)' }}>{track.artist}</p>
                    </div>
                    <span className="text-xs font-mono shrink-0" style={{ color: 'var(--text-secondary)' }}>{formatDuration(track.duration)}</span>
                    <div className="flex items-center gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
                      {i > 0 && (
                        <button onClick={() => handleMoveTrack(i, i - 1)} className="p-1 rounded hover:opacity-80"
                          style={{ color: 'var(--text-secondary)' }}><ChevronUp size={14} /></button>
                      )}
                      {i < playlist.tracks.length - 1 && (
                        <button onClick={() => handleMoveTrack(i, i + 1)} className="p-1 rounded hover:opacity-80"
                          style={{ color: 'var(--text-secondary)' }}><ChevronDown size={14} /></button>
                      )}
                      <button onClick={() => handleRemoveTrack(i)} className="p-1 rounded hover:opacity-80"
                        style={{ color: 'var(--danger)' }}><Trash2 size={14} /></button>
                    </div>
                  </div>
                ))}
              </div>
            )}
          </div>
        </div>

        {/* Right: Search */}
        <div className="flex flex-col rounded-xl overflow-hidden"
          style={{ width: '380px', minWidth: '380px', backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
          <div className="px-4 py-3" style={{ borderBottom: '1px solid var(--border)' }}>
            <div className="flex items-center gap-2 mb-3">
              <Search size={16} style={{ color: 'var(--accent)' }} />
              <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Buscar canciones</span>
            </div>
            <div className="flex gap-2">
              <input ref={searchRef} value={searchQuery} onChange={e => setSearchQuery(e.target.value)}
                onKeyDown={e => e.key === 'Enter' && handleSearch()}
                placeholder="Artista, cancion, album..."
                className="flex-1 px-3 py-2 rounded-lg text-sm outline-none"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
              <button onClick={handleSearch} disabled={searching || !searchQuery.trim()}
                className="px-3 py-2 rounded-lg text-sm font-medium hover:opacity-90"
                style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                {searching ? <Loader2 size={16} className="animate-spin" /> : <Search size={16} />}
              </button>
            </div>
          </div>
          <div className="flex-1 overflow-y-auto">
            {searching && searchResults.length === 0 && (
              <div className="flex items-center justify-center py-8 gap-2">
                <Loader2 size={20} className="animate-spin" style={{ color: 'var(--accent)' }} />
                <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>Buscando...</span>
              </div>
            )}
            {!searching && searchResults.length === 0 && (
              <div className="flex flex-col items-center justify-center h-full gap-3 py-12">
                <Search size={36} style={{ color: 'var(--text-secondary)', opacity: 0.3 }} />
                <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Busca para agregar canciones</p>
              </div>
            )}
            {searchResults.length > 0 && (
              <div className="divide-y" style={{ borderColor: 'var(--border)' }}>
                {searchResults.map(track => {
                  const alreadyAdded = playlist.tracks.some(t => t.id === track.id)
                  return (
                    <div key={track.id} className="flex items-center gap-3 px-4 py-2.5 group transition-colors hover:opacity-90"
                      style={{ backgroundColor: alreadyAdded ? 'var(--accent)' + '08' : 'transparent' }}>
                      <img src={track.thumbnail} alt="" className="w-10 h-10 rounded-lg object-cover shrink-0 cursor-pointer hover:opacity-80"
                        style={{ border: '1px solid var(--border)' }}
                        onClick={() => handlePlayTrack(track.id)} />
                      <div className="flex-1 min-w-0">
                        <p className="text-sm truncate" style={{ color: 'var(--text-primary)' }}>{track.title}</p>
                        <p className="text-xs truncate" style={{ color: 'var(--text-secondary)' }}>{track.artist} · {formatDuration(track.duration)}</p>
                      </div>
                      {loadingTrack === track.id ? (
                        <Loader2 size={16} className="animate-spin shrink-0" style={{ color: 'var(--accent)' }} />
                      ) : alreadyAdded ? (
                        <span className="text-xs px-2 py-1 rounded shrink-0" style={{ color: 'var(--success)', backgroundColor: 'var(--success)' + '15' }}>Agregada</span>
                      ) : (
                        <button onClick={() => handleAddTrack(track)}
                          className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-xs font-medium hover:opacity-90 shrink-0"
                          style={{ backgroundColor: 'var(--success)' + '20', color: 'var(--success)', border: '1px solid var(--success)' + '40' }}>
                          <Plus size={14} /> Agregar
                        </button>
                      )}
                    </div>
                  )
                })}
              </div>
            )}
          </div>
        </div>
      </div>
    </div>
  )
}
