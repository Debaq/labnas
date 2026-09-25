import { api } from './client'

// --- Notes ---

export async function fetchNotes(): Promise<import('../types/notes').Note[]> {
  const res = await api('/api/notes')
  if (!res.ok) throw new Error('Error al obtener notas')
  return res.json()
}

export async function createNote(title: string, content?: string, shared_with?: string[], is_public?: boolean): Promise<import('../types/notes').Note> {
  const res = await api('/api/notes', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ title, content: content || '', shared_with: shared_with || [], is_public: is_public || false }),
  })
  if (!res.ok) throw new Error('Error al crear nota')
  return res.json()
}

export async function updateNote(id: string, data: { title?: string; content?: string; shared_with?: string[]; is_public?: boolean }): Promise<import('../types/notes').Note> {
  const res = await api(`/api/notes/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al actualizar nota')
  return res.json()
}

export async function deleteNote(id: string): Promise<void> {
  const res = await api(`/api/notes/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar nota')
}
