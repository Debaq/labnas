import { useEffect, useState, useRef } from 'react'
import { Plus, Trash2, Save, FileText, Edit3, Loader2, X } from 'lucide-react'
import { fetchNotes, createNote, updateNote, deleteNote } from '../api'
import type { Note } from '../types/notes'

function renderMarkdown(md: string): string {
  return md
    .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;')
    .replace(/^### (.+)$/gm, '<h3>$1</h3>')
    .replace(/^## (.+)$/gm, '<h2>$1</h2>')
    .replace(/^# (.+)$/gm, '<h1>$1</h1>')
    .replace(/\*\*(.+?)\*\*/g, '<strong>$1</strong>')
    .replace(/\*(.+?)\*/g, '<em>$1</em>')
    .replace(/`(.+?)`/g, '<code style="background:var(--bg-tertiary);padding:2px 6px;border-radius:4px;font-size:0.85em">$1</code>')
    .replace(/^- (.+)$/gm, '<li>$1</li>')
    .replace(/(<li>.*<\/li>)/gs, '<ul>$1</ul>')
    .replace(/^---$/gm, '<hr style="border-color:var(--border);margin:16px 0">')
    .replace(/\[(.+?)\]\((.+?)\)/g, '<a href="$2" target="_blank" style="color:var(--accent)">$1</a>')
    .replace(/\n/g, '<br>')
}

export default function NotesPage() {
  const [notes, setNotes] = useState<Note[]>([])
  const [loading, setLoading] = useState(true)
  const [selectedNote, setSelectedNote] = useState<Note | null>(null)
  const [editing, setEditing] = useState(false)
  const [editContent, setEditContent] = useState('')
  const [editTitle, setEditTitle] = useState('')
  const [saving, setSaving] = useState(false)
  const [showNew, setShowNew] = useState(false)
  const [newTitle, setNewTitle] = useState('')
  const textareaRef = useRef<HTMLTextAreaElement>(null)

  useEffect(() => {
    loadNotes()
  }, [])

  async function loadNotes() {
    setLoading(true)
    try {
      const data = await fetchNotes()
      setNotes(data)
      if (data.length > 0 && !selectedNote) {
        setSelectedNote(data[0])
      }
    } catch {}
    finally { setLoading(false) }
  }

  async function handleCreate() {
    if (!newTitle.trim()) return
    try {
      const note = await createNote(newTitle.trim())
      setNotes([...notes, note])
      setSelectedNote(note)
      setEditing(true)
      setEditContent('')
      setEditTitle(note.title)
      setShowNew(false)
      setNewTitle('')
    } catch {}
  }

  async function handleSave() {
    if (!selectedNote) return
    setSaving(true)
    try {
      const updated = await updateNote(selectedNote.id, { title: editTitle, content: editContent })
      setNotes(notes.map(n => n.id === updated.id ? updated : n))
      setSelectedNote(updated)
      setEditing(false)
    } catch {}
    finally { setSaving(false) }
  }

  async function handleDelete(id: string) {
    if (!confirm('Eliminar esta nota?')) return
    try {
      await deleteNote(id)
      const remaining = notes.filter(n => n.id !== id)
      setNotes(remaining)
      if (selectedNote?.id === id) {
        setSelectedNote(remaining[0] || null)
        setEditing(false)
      }
    } catch {}
  }

  function startEditing(note: Note) {
    setEditing(true)
    setEditContent(note.content)
    setEditTitle(note.title)
    setTimeout(() => textareaRef.current?.focus(), 50)
  }

  return (
    <div className="flex gap-6 h-full" style={{ minHeight: 0 }}>
      {/* Sidebar - Lista de notas */}
      <div
        className="w-[260px] min-w-[260px] flex flex-col rounded-xl overflow-hidden"
        style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
      >
        <div className="px-4 py-3 flex items-center justify-between" style={{ borderBottom: '1px solid var(--border)' }}>
          <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Notas</span>
          <button
            onClick={() => setShowNew(!showNew)}
            className="p-1 rounded-lg hover:opacity-80"
            style={{ color: 'var(--accent)' }}
          >
            <Plus size={18} />
          </button>
        </div>

        {showNew && (
          <div className="px-3 py-2 flex gap-2" style={{ borderBottom: '1px solid var(--border)' }}>
            <input
              value={newTitle}
              onChange={e => setNewTitle(e.target.value)}
              onKeyDown={e => e.key === 'Enter' && handleCreate()}
              placeholder="Titulo..."
              autoFocus
              className="flex-1 px-2 py-1.5 rounded-lg text-xs outline-none"
              style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
            />
            <button onClick={handleCreate} className="text-xs px-2 rounded-lg" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>OK</button>
          </div>
        )}

        <div className="flex-1 overflow-auto">
          {loading ? (
            <div className="flex justify-center py-8"><Loader2 size={20} className="animate-spin" style={{ color: 'var(--accent)' }} /></div>
          ) : notes.length === 0 ? (
            <p className="text-xs text-center py-8" style={{ color: 'var(--text-secondary)' }}>Sin notas</p>
          ) : (
            notes.map(note => (
              <div
                key={note.id}
                onClick={() => { setSelectedNote(note); setEditing(false) }}
                className="px-4 py-3 cursor-pointer transition-all duration-200 hover:opacity-80 group"
                style={{
                  backgroundColor: selectedNote?.id === note.id ? 'var(--accent-alpha)' : 'transparent',
                  borderBottom: '1px solid var(--border)',
                }}
              >
                <div className="flex items-center justify-between">
                  <span className="text-sm font-medium truncate" style={{ color: selectedNote?.id === note.id ? 'var(--accent)' : 'var(--text-primary)' }}>
                    {note.title}
                  </span>
                  <button
                    onClick={(e) => { e.stopPropagation(); handleDelete(note.id) }}
                    className="opacity-0 group-hover:opacity-100 p-1 rounded hover:opacity-80"
                    style={{ color: 'var(--danger)' }}
                  >
                    <Trash2 size={12} />
                  </button>
                </div>
                <p className="text-[10px] mt-0.5" style={{ color: 'var(--text-secondary)' }}>
                  {note.updated_by} - {new Date(note.updated_at).toLocaleDateString('es-ES')}
                </p>
              </div>
            ))
          )}
        </div>
      </div>

      {/* Editor / Preview */}
      <div className="flex-1 flex flex-col rounded-xl overflow-hidden" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
        {selectedNote ? (
          <>
            {/* Toolbar */}
            <div className="px-4 py-3 flex items-center justify-between" style={{ borderBottom: '1px solid var(--border)' }}>
              {editing ? (
                <input
                  value={editTitle}
                  onChange={e => setEditTitle(e.target.value)}
                  className="text-sm font-semibold bg-transparent outline-none flex-1"
                  style={{ color: 'var(--text-primary)' }}
                />
              ) : (
                <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>
                  {selectedNote.title}
                </span>
              )}
              <div className="flex items-center gap-2">
                {editing ? (
                  <>
                    <button
                      onClick={() => setEditing(false)}
                      className="flex items-center gap-1 px-2 py-1 rounded-lg text-xs"
                      style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
                    >
                      <X size={12} />Cancelar
                    </button>
                    <button
                      onClick={handleSave}
                      disabled={saving}
                      className="flex items-center gap-1 px-3 py-1 rounded-lg text-xs font-medium"
                      style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
                    >
                      {saving ? <Loader2 size={12} className="animate-spin" /> : <Save size={12} />}
                      Guardar
                    </button>
                  </>
                ) : (
                  <button
                    onClick={() => startEditing(selectedNote)}
                    className="flex items-center gap-1 px-3 py-1 rounded-lg text-xs font-medium"
                    style={{ color: 'var(--accent)', border: '1px solid var(--accent)' }}
                  >
                    <Edit3 size={12} />Editar
                  </button>
                )}
              </div>
            </div>

            {/* Content */}
            <div className="flex-1 overflow-auto">
              {editing ? (
                <div className="flex h-full">
                  {/* Editor */}
                  <textarea
                    ref={textareaRef}
                    value={editContent}
                    onChange={e => setEditContent(e.target.value)}
                    className="flex-1 p-4 text-sm font-mono outline-none resize-none"
                    style={{
                      backgroundColor: 'var(--bg-primary)',
                      color: 'var(--text-primary)',
                      borderRight: '1px solid var(--border)',
                    }}
                    placeholder="Escribe en Markdown..."
                  />
                  {/* Live preview */}
                  <div
                    className="flex-1 p-4 text-sm overflow-auto"
                    style={{ color: 'var(--text-primary)' }}
                    dangerouslySetInnerHTML={{ __html: renderMarkdown(editContent) }}
                  />
                </div>
              ) : (
                <div className="p-6">
                  {selectedNote.content ? (
                    <div
                      className="text-sm leading-relaxed"
                      style={{ color: 'var(--text-primary)' }}
                      dangerouslySetInnerHTML={{ __html: renderMarkdown(selectedNote.content) }}
                    />
                  ) : (
                    <div className="text-center py-12">
                      <FileText size={40} className="mx-auto mb-3" style={{ color: 'var(--text-secondary)' }} />
                      <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Nota vacia. Toca "Editar" para escribir.</p>
                    </div>
                  )}
                </div>
              )}
            </div>

            {/* Footer */}
            <div className="px-4 py-2 text-[10px] flex justify-between" style={{ borderTop: '1px solid var(--border)', color: 'var(--text-secondary)' }}>
              <span>Por: {selectedNote.created_by}</span>
              <span>Editado: {new Date(selectedNote.updated_at).toLocaleString('es-ES')} por {selectedNote.updated_by}</span>
            </div>
          </>
        ) : (
          <div className="flex-1 flex items-center justify-center">
            <div className="text-center">
              <FileText size={48} className="mx-auto mb-4" style={{ color: 'var(--text-secondary)' }} />
              <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Selecciona o crea una nota</p>
            </div>
          </div>
        )}
      </div>
    </div>
  )
}
