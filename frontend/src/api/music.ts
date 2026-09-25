import { api } from './client'

// --- Music ---

export interface MusicTrack {
  id: string
  title: string
  artist: string
  thumbnail: string
  duration: number
  added_by: string | null
}

export interface MusicState {
  current: MusicTrack | null
  queue: MusicTrack[]
  started_by: string | null
  history: { id: string; title: string; artist: string; thumbnail: string; played_by: string }[]
  paused: boolean
  volume: number
  repeat: 'off' | 'all' | 'one'
  shuffle: boolean
  video: boolean
  video_screen: number | null
  elapsed: number
}

export interface PlaylistTrack {
  id: string; title: string; artist: string; thumbnail: string; duration: number; added_by: string
}
export interface Playlist {
  id: string; name: string; description: string; created_by: string
  tracks: PlaylistTrack[]; created_at: string; updated_at: string
}

export async function fetchPlaylists(): Promise<Playlist[]> {
  const res = await api('/api/music/playlists'); return res.ok ? res.json() : []
}
export async function createPlaylist(name: string, description?: string): Promise<Playlist> {
  const res = await api('/api/music/playlists', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, description: description || '' }) })
  if (!res.ok) throw new Error('Error al crear playlist'); return res.json()
}
export async function updatePlaylist(id: string, data: { name?: string; description?: string }): Promise<Playlist> {
  const res = await api(`/api/music/playlists/${id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(data) })
  if (!res.ok) throw new Error('Error al actualizar playlist'); return res.json()
}
export async function deletePlaylist(id: string): Promise<void> {
  await api(`/api/music/playlists/${id}`, { method: 'DELETE' })
}
export async function addTrackToPlaylist(playlistId: string, track: { id: string; title: string; artist: string; thumbnail: string; duration: number }): Promise<Playlist> {
  const res = await api(`/api/music/playlists/${playlistId}/tracks`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(track) })
  if (!res.ok) throw new Error('Error al agregar track'); return res.json()
}
export async function removeTrackFromPlaylist(playlistId: string, index: number): Promise<Playlist> {
  const res = await api(`/api/music/playlists/${playlistId}/tracks/${index}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar track'); return res.json()
}
export async function moveTrackInPlaylist(playlistId: string, from: number, to: number): Promise<Playlist> {
  const res = await api(`/api/music/playlists/${playlistId}/tracks/move`, { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ from, to }) })
  if (!res.ok) throw new Error('Error al mover track'); return res.json()
}
export async function loadPlaylist(playlistId: string): Promise<void> {
  const res = await api(`/api/music/playlists/${playlistId}/load`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al cargar playlist')
}
export async function saveQueueAsPlaylist(name: string, description?: string): Promise<Playlist> {
  const res = await api('/api/music/playlists/save-queue', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify({ name, description: description || '' }) })
  if (!res.ok) throw new Error('Error al guardar playlist'); return res.json()
}

export async function searchMusic(q: string): Promise<MusicTrack[]> {
  const res = await api(`/api/music/search?q=${encodeURIComponent(q)}`)
  if (!res.ok) throw new Error('Error buscando musica')
  return res.json()
}

export async function playMusic(id: string, force?: boolean): Promise<MusicState> {
  const res = await api('/api/music/play', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ id, force: force || false }),
  })
  if (!res.ok) throw new Error('Error reproduciendo')
  return res.json()
}

export async function nextMusic(): Promise<MusicState> {
  const res = await api('/api/music/next', { method: 'POST' })
  if (!res.ok) throw new Error('Error pasando cancion')
  return res.json()
}

export async function getCurrentMusic(): Promise<MusicState> {
  const res = await api('/api/music/current')
  if (!res.ok) throw new Error('Error obteniendo estado')
  return res.json()
}

export async function stopMusic(): Promise<MusicState> {
  const res = await api('/api/music/stop', { method: 'POST' })
  if (!res.ok) throw new Error('Error deteniendo')
  return res.json()
}

export async function pauseMusic(): Promise<MusicState> {
  const res = await api('/api/music/pause', { method: 'POST' })
  if (!res.ok) throw new Error('Error pausando')
  return res.json()
}

export async function previousMusic(): Promise<MusicState> {
  const res = await api('/api/music/previous', { method: 'POST' })
  if (!res.ok) { const t = await res.text(); throw new Error(t) }
  return res.json()
}

export async function setMusicVolume(volume: number): Promise<MusicState> {
  const res = await api('/api/music/volume', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ volume }),
  })
  if (!res.ok) throw new Error('Error ajustando volumen')
  return res.json()
}

export async function playFromQueue(index: number): Promise<MusicState> {
  const res = await api(`/api/music/queue/play/${index}`, { method: 'POST' })
  if (!res.ok) throw new Error('Error reproduciendo de cola')
  return res.json()
}

export async function moveInQueue(from: number, to: number): Promise<MusicState> {
  const res = await api('/api/music/queue/move', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ from, to }),
  })
  if (!res.ok) throw new Error('Error moviendo en cola')
  return res.json()
}

export async function toggleShuffle(): Promise<MusicState> {
  const res = await api('/api/music/shuffle', { method: 'POST' })
  if (!res.ok) throw new Error('Error')
  return res.json()
}

export async function toggleRepeat(): Promise<MusicState> {
  const res = await api('/api/music/repeat', { method: 'POST' })
  if (!res.ok) throw new Error('Error')
  return res.json()
}

export interface ScreenInfo {
  index: number
  connector: string
  name: string
  connected: boolean
}

export async function getScreens(): Promise<ScreenInfo[]> {
  const res = await api('/api/music/screens')
  if (!res.ok) return []
  return res.json()
}

export async function setMusicVideo(video: boolean, screen?: number): Promise<MusicState> {
  const res = await api('/api/music/video', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ video, screen }),
  })
  if (!res.ok) throw new Error('Error')
  return res.json()
}

export async function removeFromQueue(index: number): Promise<MusicState> {
  const res = await api('/api/music/queue', {
    method: 'DELETE',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ index }),
  })
  if (!res.ok) throw new Error('Error eliminando de cola')
  return res.json()
}

export async function recommendMusic(): Promise<MusicState> {
  const res = await api('/api/music/recommend', { method: 'POST' })
  if (!res.ok) { const t = await res.text(); throw new Error(t) }
  return res.json()
}

export async function getMpvArgs(): Promise<string[]> {
  const res = await api('/api/music/mpv-args')
  if (!res.ok) throw new Error('Error obteniendo args de mpv')
  return res.json()
}

export async function setMpvArgs(args: string[]): Promise<string[]> {
  const res = await api('/api/music/mpv-args', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(args),
  })
  if (!res.ok) throw new Error('Error guardando args de mpv')
  return res.json()
}

export async function setLastfmKey(key: string): Promise<string> {
  const res = await api('/api/music/lastfm-key', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ key }),
  })
  if (!res.ok) throw new Error('Error guardando API key de Last.fm')
  return res.text()
}

export async function clearQueue(): Promise<MusicState> {
  const res = await api('/api/music/queue/clear', { method: 'POST' })
  if (!res.ok) throw new Error('Error vaciando cola')
  return res.json()
}

export async function luckyPlay(artist: string, track: string): Promise<MusicState> {
  const res = await api('/api/music/lucky', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ artist, track }),
  })
  if (!res.ok) {
    const text = await res.text()
    throw new Error(text || 'Error con suerte')
  }
  return res.json()
}

export async function startRadio(artist: string, track: string): Promise<MusicState> {
  const res = await api('/api/music/radio', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ artist, track }),
  })
  if (!res.ok) {
    const text = await res.text()
    throw new Error(text || 'Error iniciando radio')
  }
  return res.json()
}
