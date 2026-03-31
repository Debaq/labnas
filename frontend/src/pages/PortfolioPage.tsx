import { useEffect, useState, useCallback } from 'react'
import {
  Plus, Trash2, Pencil, X, Loader2, Search,
  ChevronDown, ChevronUp, Check, Calendar, Users,
  Microscope, BookOpen, GraduationCap, Wrench,
  FolderOpen, Tag, FileText, DollarSign, Clock,
  MapPin, CheckSquare, Square,
} from 'lucide-react'
import {
  fetchPortfolio, createPortfolioEntry, updatePortfolioEntry, deletePortfolioEntry,
  togglePortfolioRequirement, togglePortfolioMilestone, fetchUsernames,
} from '../api'
import type { PortfolioEntry, PortfolioType, PortfolioStatus, PortfolioScope, PortfolioRequirement, PortfolioMilestone } from '../types'

const TYPE_CONFIG: Record<PortfolioType, { label: string; icon: typeof Microscope }> = {
  project: { label: 'Proyecto', icon: Microscope },
  course: { label: 'Curso', icon: BookOpen },
  diploma: { label: 'Diplomado', icon: GraduationCap },
  workshop: { label: 'Taller', icon: Wrench },
}

const STATUS_CONFIG: Record<PortfolioStatus, { label: string; color: string }> = {
  planned: { label: 'Planificado', color: 'var(--text-secondary)' },
  submitted: { label: 'Postulado', color: 'var(--warning)' },
  active: { label: 'Activo', color: 'var(--accent)' },
  completed: { label: 'Completado', color: 'var(--success)' },
  cancelled: { label: 'Cancelado', color: 'var(--danger)' },
}

const SCOPE_CONFIG: Record<PortfolioScope, { label: string; color: string }> = {
  own: { label: 'Nuestro', color: 'var(--accent)' },
  external: { label: 'Externo / Referencia', color: 'var(--warning)' },
  historic: { label: 'Historico', color: 'var(--text-secondary)' },
}

const MODALITIES = ['Presencial', 'Online', 'Hibrido']

const emptyForm = (): Partial<PortfolioEntry> => ({
  entry_type: 'project',
  scope: 'own',
  title: '',
  description: '',
  institution: '',
  url: '',
  contact: '',
  funding_source: '',
  budget: null,
  principal_investigator: '',
  collaborators: [],
  participants: [],
  start_date: '',
  end_date: null,
  hours: null,
  modality: 'Presencial',
  status: 'planned',
  requirements: [],
  milestones: [],
  tags: [],
  notes: '',
  related_files: [],
  related_inventory: [],
})

export default function PortfolioPage() {
  const [entries, setEntries] = useState<PortfolioEntry[]>([])
  const [loading, setLoading] = useState(true)
  const [filterType, setFilterType] = useState<PortfolioType | 'all'>('all')
  const [filterStatus, setFilterStatus] = useState<PortfolioStatus | 'all'>('all')
  const [filterScope, setFilterScope] = useState<PortfolioScope | 'all'>('all')
  const [searchQuery, setSearchQuery] = useState('')
  const [expandedEntry, setExpandedEntry] = useState<string | null>(null)
  const [showModal, setShowModal] = useState(false)
  const [editingEntry, setEditingEntry] = useState<PortfolioEntry | null>(null)
  const [form, setForm] = useState<Partial<PortfolioEntry>>(emptyForm())
  const [usernames, setUsernames] = useState<string[]>([])
  const [tagInput, setTagInput] = useState('')
  const [collaboratorSearch, setCollaboratorSearch] = useState('')
  const [participantSearch, setParticipantSearch] = useState('')
  const [modalSection, setModalSection] = useState(0)

  const loadData = useCallback(async () => {
    setLoading(true)
    try {
      const [data, users] = await Promise.all([fetchPortfolio(), fetchUsernames()])
      setEntries(data)
      setUsernames(users)
    } catch {} finally { setLoading(false) }
  }, [])

  useEffect(() => { loadData() }, [loadData])

  const filtered = entries.filter(e => {
    if (filterType !== 'all' && e.entry_type !== filterType) return false
    if (filterStatus !== 'all' && e.status !== filterStatus) return false
    if (filterScope !== 'all' && (e.scope || 'own') !== filterScope) return false
    if (searchQuery) {
      const q = searchQuery.toLowerCase()
      return e.title.toLowerCase().includes(q) ||
        e.institution.toLowerCase().includes(q) ||
        e.description.toLowerCase().includes(q) ||
        e.principal_investigator.toLowerCase().includes(q) ||
        e.tags.some(t => t.toLowerCase().includes(q))
    }
    return true
  })

  const typeCounts = (type: PortfolioType | 'all') => {
    if (type === 'all') return entries.length
    return entries.filter(e => e.entry_type === type).length
  }

  const statusCounts = (status: PortfolioStatus | 'all') => {
    if (status === 'all') return entries.length
    return entries.filter(e => e.status === status).length
  }

  function openNew() {
    setEditingEntry(null)
    setForm(emptyForm())
    setModalSection(0)
    setTagInput('')
    setCollaboratorSearch('')
    setParticipantSearch('')
    setShowModal(true)
  }

  function openEdit(entry: PortfolioEntry) {
    setEditingEntry(entry)
    setForm({ ...entry })
    setModalSection(0)
    setTagInput('')
    setCollaboratorSearch('')
    setParticipantSearch('')
    setShowModal(true)
  }

  async function handleSave() {
    if (!form.title?.trim()) return
    try {
      if (editingEntry) {
        await updatePortfolioEntry(editingEntry.id, form)
      } else {
        await createPortfolioEntry(form)
      }
      setShowModal(false)
      await loadData()
    } catch (e: any) { alert(e.message) }
  }

  async function handleDelete(id: string) {
    if (!confirm('Eliminar esta ficha del portafolio?')) return
    await deletePortfolioEntry(id)
    await loadData()
  }

  async function handleToggleReq(entryId: string, reqId: string) {
    try {
      const updated = await togglePortfolioRequirement(entryId, reqId)
      setEntries(prev => prev.map(e => e.id === updated.id ? updated : e))
    } catch {}
  }

  async function handleToggleMil(entryId: string, milId: string) {
    try {
      const updated = await togglePortfolioMilestone(entryId, milId)
      setEntries(prev => prev.map(e => e.id === updated.id ? updated : e))
    } catch {}
  }

  // Form helpers
  function addRequirement() {
    const reqs = [...(form.requirements || [])]
    reqs.push({ id: crypto.randomUUID().slice(0, 8), text: '', completed: false })
    setForm({ ...form, requirements: reqs })
  }

  function removeRequirement(idx: number) {
    const reqs = [...(form.requirements || [])]
    reqs.splice(idx, 1)
    setForm({ ...form, requirements: reqs })
  }

  function updateRequirement(idx: number, text: string) {
    const reqs = [...(form.requirements || [])]
    reqs[idx] = { ...reqs[idx], text }
    setForm({ ...form, requirements: reqs })
  }

  function addMilestone() {
    const mils = [...(form.milestones || [])]
    mils.push({ id: crypto.randomUUID().slice(0, 8), title: '', date: '', completed: false })
    setForm({ ...form, milestones: mils })
  }

  function removeMilestone(idx: number) {
    const mils = [...(form.milestones || [])]
    mils.splice(idx, 1)
    setForm({ ...form, milestones: mils })
  }

  function updateMilestone(idx: number, field: 'title' | 'date', value: string) {
    const mils = [...(form.milestones || [])]
    mils[idx] = { ...mils[idx], [field]: value }
    setForm({ ...form, milestones: mils })
  }

  function addTag() {
    if (!tagInput.trim()) return
    const tags = [...(form.tags || [])]
    if (!tags.includes(tagInput.trim())) tags.push(tagInput.trim())
    setForm({ ...form, tags })
    setTagInput('')
  }

  function removeTag(tag: string) {
    setForm({ ...form, tags: (form.tags || []).filter(t => t !== tag) })
  }

  function addCollaborator(name: string) {
    const colls = [...(form.collaborators || [])]
    if (!colls.includes(name)) colls.push(name)
    setForm({ ...form, collaborators: colls })
    setCollaboratorSearch('')
  }

  function removeCollaborator(name: string) {
    setForm({ ...form, collaborators: (form.collaborators || []).filter(c => c !== name) })
  }

  function addParticipant(name: string) {
    const parts = [...(form.participants || [])]
    if (!parts.includes(name)) parts.push(name)
    setForm({ ...form, participants: parts })
    setParticipantSearch('')
  }

  function removeParticipant(name: string) {
    setForm({ ...form, participants: (form.participants || []).filter(p => p !== name) })
  }

  function progressPercent(entry: PortfolioEntry): number {
    if (entry.requirements.length === 0) return 0
    const done = entry.requirements.filter(r => r.completed).length
    return Math.round((done / entry.requirements.length) * 100)
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <Loader2 className="animate-spin" size={32} style={{ color: 'var(--accent)' }} />
      </div>
    )
  }

  const TypeIcon = ({ type, size = 18 }: { type: PortfolioType; size?: number }) => {
    const Ic = TYPE_CONFIG[type]?.icon || Microscope
    return <Ic size={size} />
  }

  return (
    <div className="flex gap-6 h-full">
      {/* Panel izquierdo - Filtros */}
      <div
        className="flex-shrink-0 flex flex-col gap-4"
        style={{ width: 240 }}
      >
        {/* Busqueda */}
        <div className="relative">
          <Search size={16} className="absolute left-3 top-1/2 -translate-y-1/2" style={{ color: 'var(--text-secondary)' }} />
          <input
            type="text"
            placeholder="Buscar..."
            value={searchQuery}
            onChange={e => setSearchQuery(e.target.value)}
            className="w-full pl-9 pr-3 py-2 rounded-lg text-sm"
            style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
          />
        </div>

        {/* Filtros por tipo */}
        <div
          className="rounded-xl p-4"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <h3 className="text-xs font-semibold uppercase mb-3" style={{ color: 'var(--text-secondary)' }}>Tipo</h3>
          <div className="space-y-1">
            <button
              onClick={() => setFilterType('all')}
              className="w-full flex items-center justify-between px-3 py-2 rounded-lg text-sm transition-all"
              style={{
                backgroundColor: filterType === 'all' ? 'var(--accent-alpha)' : 'transparent',
                color: filterType === 'all' ? 'var(--accent)' : 'var(--text-primary)',
              }}
            >
              <span>Todo</span>
              <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>{typeCounts('all')}</span>
            </button>
            {(Object.entries(TYPE_CONFIG) as [PortfolioType, typeof TYPE_CONFIG['project']][]).map(([key, cfg]) => (
              <button
                key={key}
                onClick={() => setFilterType(key)}
                className="w-full flex items-center justify-between px-3 py-2 rounded-lg text-sm transition-all"
                style={{
                  backgroundColor: filterType === key ? 'var(--accent-alpha)' : 'transparent',
                  color: filterType === key ? 'var(--accent)' : 'var(--text-primary)',
                }}
              >
                <span className="flex items-center gap-2">
                  <cfg.icon size={14} />
                  {cfg.label + 's'}
                </span>
                <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>{typeCounts(key)}</span>
              </button>
            ))}
          </div>
        </div>

        {/* Filtros por estado */}
        <div
          className="rounded-xl p-4"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <h3 className="text-xs font-semibold uppercase mb-3" style={{ color: 'var(--text-secondary)' }}>Estado</h3>
          <div className="space-y-1">
            <button
              onClick={() => setFilterStatus('all')}
              className="w-full flex items-center justify-between px-3 py-2 rounded-lg text-sm transition-all"
              style={{
                backgroundColor: filterStatus === 'all' ? 'var(--accent-alpha)' : 'transparent',
                color: filterStatus === 'all' ? 'var(--accent)' : 'var(--text-primary)',
              }}
            >
              <span>Todos</span>
              <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>{statusCounts('all')}</span>
            </button>
            {(Object.entries(STATUS_CONFIG) as [PortfolioStatus, typeof STATUS_CONFIG['planned']][]).map(([key, cfg]) => (
              <button
                key={key}
                onClick={() => setFilterStatus(key)}
                className="w-full flex items-center justify-between px-3 py-2 rounded-lg text-sm transition-all"
                style={{
                  backgroundColor: filterStatus === key ? 'var(--accent-alpha)' : 'transparent',
                  color: filterStatus === key ? 'var(--accent)' : 'var(--text-primary)',
                }}
              >
                <span className="flex items-center gap-2">
                  <span className="w-2 h-2 rounded-full" style={{ backgroundColor: cfg.color }} />
                  {cfg.label}
                </span>
                <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>{statusCounts(key)}</span>
              </button>
            ))}
          </div>
        </div>

        {/* Filtros por alcance */}
        <div className="rounded-xl p-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
          <h3 className="text-xs font-semibold uppercase mb-3" style={{ color: 'var(--text-secondary)' }}>Alcance</h3>
          <div className="space-y-1">
            <button onClick={() => setFilterScope('all')}
              className="w-full flex items-center justify-between px-3 py-2 rounded-lg text-sm transition-all"
              style={{ backgroundColor: filterScope === 'all' ? 'var(--accent-alpha)' : 'transparent', color: filterScope === 'all' ? 'var(--accent)' : 'var(--text-primary)' }}>
              <span>Todos</span>
            </button>
            {(Object.entries(SCOPE_CONFIG) as [PortfolioScope, typeof SCOPE_CONFIG['own']][]).map(([key, cfg]) => (
              <button key={key} onClick={() => setFilterScope(key)}
                className="w-full flex items-center justify-between px-3 py-2 rounded-lg text-sm transition-all"
                style={{ backgroundColor: filterScope === key ? 'var(--accent-alpha)' : 'transparent', color: filterScope === key ? 'var(--accent)' : 'var(--text-primary)' }}>
                <span className="flex items-center gap-2">
                  <span className="w-2 h-2 rounded-full" style={{ backgroundColor: cfg.color }} />
                  {cfg.label}
                </span>
              </button>
            ))}
          </div>
        </div>

        {/* Boton nueva ficha */}
        <button
          onClick={openNew}
          className="flex items-center justify-center gap-2 px-4 py-3 rounded-xl text-sm font-medium transition-all hover:opacity-90"
          style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
        >
          <Plus size={16} /> Nueva Ficha
        </button>
      </div>

      {/* Panel derecho - Lista de fichas */}
      <div className="flex-1 min-w-0 space-y-3 overflow-auto">
        {filtered.length === 0 ? (
          <div className="flex flex-col items-center justify-center h-64 gap-3" style={{ color: 'var(--text-secondary)' }}>
            <GraduationCap size={48} strokeWidth={1} />
            <p className="text-sm">
              {entries.length === 0 ? 'No hay fichas en el portafolio' : 'No se encontraron resultados'}
            </p>
          </div>
        ) : filtered.map(entry => {
          const isExpanded = expandedEntry === entry.id
          const progress = progressPercent(entry)
          const stCfg = STATUS_CONFIG[entry.status]
          const tyCfg = TYPE_CONFIG[entry.entry_type]

          return (
            <div
              key={entry.id}
              className="rounded-xl transition-all"
              style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
            >
              {/* Cabecera colapsada */}
              <div
                className="flex items-center gap-4 px-5 py-4 cursor-pointer hover:opacity-90 transition-opacity"
                onClick={() => setExpandedEntry(isExpanded ? null : entry.id)}
              >
                <div className="flex-shrink-0" style={{ color: 'var(--accent)' }}>
                  <TypeIcon type={entry.entry_type} size={22} />
                </div>
                <div className="flex-1 min-w-0">
                  <div className="flex items-center gap-3">
                    <h3 className="text-sm font-semibold truncate" style={{ color: 'var(--text-primary)' }}>
                      {entry.title}
                    </h3>
                    <span className="text-[11px] px-2 py-0.5 rounded-full font-medium flex-shrink-0"
                      style={{ backgroundColor: stCfg.color + '22', color: stCfg.color }}>
                      {stCfg.label}
                    </span>
                    {entry.scope && entry.scope !== 'own' && (
                      <span className="text-[10px] px-1.5 py-0.5 rounded-full font-medium flex-shrink-0"
                        style={{ backgroundColor: SCOPE_CONFIG[entry.scope]?.color + '22', color: SCOPE_CONFIG[entry.scope]?.color }}>
                        {SCOPE_CONFIG[entry.scope]?.label}
                      </span>
                    )}
                  </div>
                  <div className="flex items-center gap-4 mt-1">
                    {entry.institution && (
                      <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                        {entry.institution}
                      </span>
                    )}
                    {entry.start_date && (
                      <span className="text-xs flex items-center gap-1" style={{ color: 'var(--text-secondary)' }}>
                        <Calendar size={10} />
                        {entry.start_date}{entry.end_date ? ` - ${entry.end_date}` : ''}
                      </span>
                    )}
                    {entry.requirements.length > 0 && (
                      <span className="text-xs flex items-center gap-1" style={{ color: 'var(--text-secondary)' }}>
                        <CheckSquare size={10} />
                        {entry.requirements.filter(r => r.completed).length}/{entry.requirements.length}
                      </span>
                    )}
                  </div>
                </div>
                {/* Barra de progreso mini */}
                {entry.requirements.length > 0 && (
                  <div className="flex-shrink-0 w-20">
                    <div className="w-full h-1.5 rounded-full" style={{ backgroundColor: 'var(--border)' }}>
                      <div
                        className="h-full rounded-full transition-all"
                        style={{ width: `${progress}%`, backgroundColor: progress === 100 ? 'var(--success)' : 'var(--accent)' }}
                      />
                    </div>
                    <span className="text-[10px] mt-0.5 block text-right" style={{ color: 'var(--text-secondary)' }}>{progress}%</span>
                  </div>
                )}
                <div style={{ color: 'var(--text-secondary)' }}>
                  {isExpanded ? <ChevronUp size={18} /> : <ChevronDown size={18} />}
                </div>
              </div>

              {/* Vista expandida */}
              {isExpanded && (
                <div className="px-5 pb-5 space-y-4" style={{ borderTop: '1px solid var(--border)' }}>
                  <div className="pt-4 grid grid-cols-2 gap-4">
                    {/* Descripcion */}
                    {entry.description && (
                      <div className="col-span-2">
                        <label className="text-[11px] font-medium uppercase" style={{ color: 'var(--text-secondary)' }}>Descripcion</label>
                        <p className="text-sm mt-1 whitespace-pre-wrap" style={{ color: 'var(--text-primary)' }}>{entry.description}</p>
                      </div>
                    )}

                    {/* Info basica */}
                    <InfoField icon={<Microscope size={12} />} label="Tipo" value={tyCfg.label} />
                    <InfoField icon={<MapPin size={12} />} label="Modalidad" value={entry.modality} />
                    {entry.principal_investigator && (
                      <InfoField icon={<Users size={12} />} label="Investigador Principal" value={entry.principal_investigator} />
                    )}
                    {entry.funding_source && (
                      <InfoField icon={<DollarSign size={12} />} label="Financiamiento" value={entry.funding_source} />
                    )}
                    {entry.budget != null && (
                      <InfoField icon={<DollarSign size={12} />} label="Presupuesto" value={`$${entry.budget.toLocaleString()}`} />
                    )}
                    {entry.hours != null && (
                      <InfoField icon={<Clock size={12} />} label="Horas" value={`${entry.hours} hrs`} />
                    )}

                    {/* Colaboradores */}
                    {entry.collaborators.length > 0 && (
                      <div className="col-span-2">
                        <label className="text-[11px] font-medium uppercase" style={{ color: 'var(--text-secondary)' }}>Colaboradores</label>
                        <div className="flex flex-wrap gap-1.5 mt-1">
                          {entry.collaborators.map(c => (
                            <span key={c} className="text-xs px-2 py-1 rounded-full" style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}>{c}</span>
                          ))}
                        </div>
                      </div>
                    )}

                    {/* Participantes */}
                    {entry.participants.length > 0 && (
                      <div className="col-span-2">
                        <label className="text-[11px] font-medium uppercase" style={{ color: 'var(--text-secondary)' }}>Participantes</label>
                        <div className="flex flex-wrap gap-1.5 mt-1">
                          {entry.participants.map(p => (
                            <span key={p} className="text-xs px-2 py-1 rounded-full" style={{ backgroundColor: 'var(--bg-secondary)', color: 'var(--text-primary)' }}>{p}</span>
                          ))}
                        </div>
                      </div>
                    )}
                  </div>

                  {/* Requisitos */}
                  {entry.requirements.length > 0 && (
                    <div>
                      <label className="text-[11px] font-medium uppercase" style={{ color: 'var(--text-secondary)' }}>
                        Requisitos ({entry.requirements.filter(r => r.completed).length}/{entry.requirements.length})
                      </label>
                      <div className="space-y-1 mt-2">
                        {entry.requirements.map(req => (
                          <div
                            key={req.id}
                            className="flex items-center gap-2 cursor-pointer group"
                            onClick={() => handleToggleReq(entry.id, req.id)}
                          >
                            {req.completed ? (
                              <CheckSquare size={16} style={{ color: 'var(--success)' }} />
                            ) : (
                              <Square size={16} style={{ color: 'var(--text-secondary)' }} />
                            )}
                            <span
                              className="text-sm"
                              style={{
                                color: req.completed ? 'var(--text-secondary)' : 'var(--text-primary)',
                                textDecoration: req.completed ? 'line-through' : 'none',
                              }}
                            >
                              {req.text}
                            </span>
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Hitos */}
                  {entry.milestones.length > 0 && (
                    <div>
                      <label className="text-[11px] font-medium uppercase" style={{ color: 'var(--text-secondary)' }}>Hitos</label>
                      <div className="space-y-1 mt-2">
                        {entry.milestones.map(mil => (
                          <div
                            key={mil.id}
                            className="flex items-center gap-2 cursor-pointer"
                            onClick={() => handleToggleMil(entry.id, mil.id)}
                          >
                            {mil.completed ? (
                              <CheckSquare size={16} style={{ color: 'var(--success)' }} />
                            ) : (
                              <Square size={16} style={{ color: 'var(--text-secondary)' }} />
                            )}
                            <span
                              className="text-sm flex-1"
                              style={{
                                color: mil.completed ? 'var(--text-secondary)' : 'var(--text-primary)',
                                textDecoration: mil.completed ? 'line-through' : 'none',
                              }}
                            >
                              {mil.title}
                            </span>
                            {mil.date && (
                              <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>{mil.date}</span>
                            )}
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Tags */}
                  {entry.tags.length > 0 && (
                    <div>
                      <label className="text-[11px] font-medium uppercase" style={{ color: 'var(--text-secondary)' }}>Tags</label>
                      <div className="flex flex-wrap gap-1.5 mt-1">
                        {entry.tags.map(tag => (
                          <span key={tag} className="text-xs px-2 py-1 rounded-full" style={{ backgroundColor: 'var(--bg-secondary)', color: 'var(--text-secondary)' }}>
                            <Tag size={10} className="inline mr-1" />{tag}
                          </span>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Archivos relacionados */}
                  {entry.related_files.length > 0 && (
                    <div>
                      <label className="text-[11px] font-medium uppercase" style={{ color: 'var(--text-secondary)' }}>Archivos Relacionados</label>
                      <div className="space-y-1 mt-1">
                        {entry.related_files.map((f, i) => (
                          <div key={i} className="flex items-center gap-2 text-xs" style={{ color: 'var(--accent)' }}>
                            <FolderOpen size={12} /> {f}
                          </div>
                        ))}
                      </div>
                    </div>
                  )}

                  {/* Notas */}
                  {entry.notes && (
                    <div>
                      <label className="text-[11px] font-medium uppercase" style={{ color: 'var(--text-secondary)' }}>Notas</label>
                      <p className="text-sm mt-1 whitespace-pre-wrap" style={{ color: 'var(--text-primary)' }}>{entry.notes}</p>
                    </div>
                  )}

                  {/* Acciones */}
                  <div className="flex items-center gap-2 pt-2" style={{ borderTop: '1px solid var(--border)' }}>
                    <button
                      onClick={() => openEdit(entry)}
                      className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all hover:opacity-80"
                      style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}
                    >
                      <Pencil size={12} /> Editar
                    </button>
                    <button
                      onClick={() => handleDelete(entry.id)}
                      className="flex items-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium transition-all hover:opacity-80"
                      style={{ backgroundColor: 'var(--danger)22', color: 'var(--danger)' }}
                    >
                      <Trash2 size={12} /> Eliminar
                    </button>
                  </div>
                </div>
              )}
            </div>
          )
        })}
      </div>

      {/* Modal crear/editar */}
      {showModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center" style={{ backgroundColor: 'rgba(0,0,0,0.5)' }}>
          <div
            className="w-full max-w-2xl max-h-[90vh] overflow-auto rounded-2xl shadow-2xl"
            style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
          >
            {/* Header modal */}
            <div className="flex items-center justify-between px-6 py-4" style={{ borderBottom: '1px solid var(--border)' }}>
              <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
                {editingEntry ? 'Editar Ficha' : 'Nueva Ficha'}
              </h2>
              <button onClick={() => setShowModal(false)} style={{ color: 'var(--text-secondary)' }}>
                <X size={20} />
              </button>
            </div>

            {/* Tabs de seccion */}
            <div className="flex border-b px-6 gap-1" style={{ borderColor: 'var(--border)' }}>
              {['Basico', 'Fechas y Financiamiento', 'Equipo', 'Requisitos', 'Hitos', 'Extras'].map((tab, i) => (
                <button
                  key={tab}
                  onClick={() => setModalSection(i)}
                  className="px-3 py-2 text-xs font-medium transition-all"
                  style={{
                    color: modalSection === i ? 'var(--accent)' : 'var(--text-secondary)',
                    borderBottom: modalSection === i ? '2px solid var(--accent)' : '2px solid transparent',
                  }}
                >
                  {tab}
                </button>
              ))}
            </div>

            {/* Contenido del modal */}
            <div className="px-6 py-4 space-y-4">
              {/* Seccion 0: Basico */}
              {modalSection === 0 && (
                <>
                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Tipo</label>
                      <select
                        value={form.entry_type || 'project'}
                        onChange={e => setForm({ ...form, entry_type: e.target.value as PortfolioType })}
                        className="w-full px-3 py-2 rounded-lg text-sm"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                      >
                        {Object.entries(TYPE_CONFIG).map(([k, v]) => (
                          <option key={k} value={k}>{v.label}</option>
                        ))}
                      </select>
                    </div>
                    <div>
                      <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Estado</label>
                      <select
                        value={form.status || 'planned'}
                        onChange={e => setForm({ ...form, status: e.target.value as PortfolioStatus })}
                        className="w-full px-3 py-2 rounded-lg text-sm"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                      >
                        {Object.entries(STATUS_CONFIG).map(([k, v]) => (
                          <option key={k} value={k}>{v.label}</option>
                        ))}
                      </select>
                    </div>
                  </div>
                  <div>
                    <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Alcance</label>
                    <div className="flex gap-2">
                      {(Object.entries(SCOPE_CONFIG) as [PortfolioScope, typeof SCOPE_CONFIG['own']][]).map(([key, cfg]) => (
                        <button key={key} type="button" onClick={() => setForm({ ...form, scope: key })}
                          className="flex-1 py-1.5 rounded-lg text-xs font-medium transition-all"
                          style={{
                            backgroundColor: (form.scope || 'own') === key ? cfg.color + '20' : 'var(--bg-tertiary)',
                            color: (form.scope || 'own') === key ? cfg.color : 'var(--text-secondary)',
                            border: (form.scope || 'own') === key ? `2px solid ${cfg.color}` : '2px solid transparent',
                          }}>
                          {cfg.label}
                        </button>
                      ))}
                    </div>
                  </div>
                  <div>
                    <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Titulo *</label>
                    <input
                      type="text"
                      value={form.title || ''}
                      onChange={e => setForm({ ...form, title: e.target.value })}
                      placeholder="Nombre del proyecto, curso, etc."
                      className="w-full px-3 py-2 rounded-lg text-sm"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                    />
                  </div>
                  <div>
                    <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Descripcion</label>
                    <textarea
                      value={form.description || ''}
                      onChange={e => setForm({ ...form, description: e.target.value })}
                      rows={3}
                      className="w-full px-3 py-2 rounded-lg text-sm resize-none"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                    />
                  </div>
                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Institucion</label>
                      <input
                        type="text"
                        value={form.institution || ''}
                        onChange={e => setForm({ ...form, institution: e.target.value })}
                        placeholder="Universidad, organismo..."
                        className="w-full px-3 py-2 rounded-lg text-sm"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                      />
                    </div>
                    <div>
                      <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Modalidad</label>
                      <select
                        value={form.modality || 'Presencial'}
                        onChange={e => setForm({ ...form, modality: e.target.value })}
                        className="w-full px-3 py-2 rounded-lg text-sm"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                      >
                        {MODALITIES.map(m => <option key={m} value={m}>{m}</option>)}
                      </select>
                    </div>
                  </div>
                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>URL / Enlace</label>
                      <input type="text" value={form.url || ''} onChange={e => setForm({ ...form, url: e.target.value })}
                        placeholder="https://..." className="w-full px-3 py-2 rounded-lg text-sm font-mono"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                    </div>
                    <div>
                      <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Contacto</label>
                      <input type="text" value={form.contact || ''} onChange={e => setForm({ ...form, contact: e.target.value })}
                        placeholder="Persona de contacto, email..." className="w-full px-3 py-2 rounded-lg text-sm"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                    </div>
                  </div>
                </>
              )}

              {/* Seccion 1: Fechas y Financiamiento */}
              {modalSection === 1 && (
                <>
                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Fecha Inicio</label>
                      <input
                        type="date"
                        value={form.start_date || ''}
                        onChange={e => setForm({ ...form, start_date: e.target.value })}
                        className="w-full px-3 py-2 rounded-lg text-sm"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                      />
                    </div>
                    <div>
                      <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Fecha Fin</label>
                      <input
                        type="date"
                        value={form.end_date || ''}
                        onChange={e => setForm({ ...form, end_date: e.target.value || null })}
                        className="w-full px-3 py-2 rounded-lg text-sm"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                      />
                    </div>
                  </div>
                  <div className="grid grid-cols-2 gap-4">
                    <div>
                      <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Fuente de Financiamiento</label>
                      <input
                        type="text"
                        value={form.funding_source || ''}
                        onChange={e => setForm({ ...form, funding_source: e.target.value })}
                        placeholder="FONDECYT, ANID, interno..."
                        className="w-full px-3 py-2 rounded-lg text-sm"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                      />
                    </div>
                    <div>
                      <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Presupuesto ($)</label>
                      <input
                        type="number"
                        value={form.budget ?? ''}
                        onChange={e => setForm({ ...form, budget: e.target.value ? parseFloat(e.target.value) : null })}
                        placeholder="0"
                        className="w-full px-3 py-2 rounded-lg text-sm"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                      />
                    </div>
                  </div>
                  <div>
                    <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Horas (cursos/diplomados)</label>
                    <input
                      type="number"
                      value={form.hours ?? ''}
                      onChange={e => setForm({ ...form, hours: e.target.value ? parseInt(e.target.value) : null })}
                      placeholder="Total de horas"
                      className="w-full px-3 py-2 rounded-lg text-sm"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                    />
                  </div>
                </>
              )}

              {/* Seccion 2: Equipo */}
              {modalSection === 2 && (
                <>
                  <div>
                    <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Investigador Principal</label>
                    <input
                      type="text"
                      value={form.principal_investigator || ''}
                      onChange={e => setForm({ ...form, principal_investigator: e.target.value })}
                      placeholder="Nombre del PI"
                      className="w-full px-3 py-2 rounded-lg text-sm"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                    />
                  </div>

                  {/* Colaboradores */}
                  <div>
                    <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Colaboradores</label>
                    <div className="flex flex-wrap gap-1.5 mb-2">
                      {(form.collaborators || []).map(c => (
                        <span key={c} className="text-xs px-2 py-1 rounded-full flex items-center gap-1" style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}>
                          {c}
                          <button onClick={() => removeCollaborator(c)}><X size={10} /></button>
                        </span>
                      ))}
                    </div>
                    <div className="relative">
                      <input
                        type="text"
                        value={collaboratorSearch}
                        onChange={e => setCollaboratorSearch(e.target.value)}
                        onKeyDown={e => { if (e.key === 'Enter' && collaboratorSearch.trim()) { addCollaborator(collaboratorSearch.trim()); e.preventDefault() } }}
                        placeholder="Buscar o escribir nombre..."
                        className="w-full px-3 py-2 rounded-lg text-sm"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                      />
                      {collaboratorSearch && (
                        <div className="absolute z-10 w-full mt-1 rounded-lg shadow-lg max-h-32 overflow-auto" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--border)' }}>
                          {usernames.filter(u => u.toLowerCase().includes(collaboratorSearch.toLowerCase()) && !(form.collaborators || []).includes(u)).map(u => (
                            <button key={u} onClick={() => addCollaborator(u)} className="w-full text-left px-3 py-2 text-sm hover:opacity-80" style={{ color: 'var(--text-primary)' }}>{u}</button>
                          ))}
                          {collaboratorSearch.trim() && !usernames.includes(collaboratorSearch.trim()) && (
                            <button onClick={() => addCollaborator(collaboratorSearch.trim())} className="w-full text-left px-3 py-2 text-sm hover:opacity-80" style={{ color: 'var(--accent)' }}>
                              + Agregar "{collaboratorSearch.trim()}"
                            </button>
                          )}
                        </div>
                      )}
                    </div>
                  </div>

                  {/* Participantes */}
                  <div>
                    <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Participantes</label>
                    <div className="flex flex-wrap gap-1.5 mb-2">
                      {(form.participants || []).map(p => (
                        <span key={p} className="text-xs px-2 py-1 rounded-full flex items-center gap-1" style={{ backgroundColor: 'var(--bg-secondary)', color: 'var(--text-primary)' }}>
                          {p}
                          <button onClick={() => removeParticipant(p)}><X size={10} /></button>
                        </span>
                      ))}
                    </div>
                    <div className="relative">
                      <input
                        type="text"
                        value={participantSearch}
                        onChange={e => setParticipantSearch(e.target.value)}
                        onKeyDown={e => { if (e.key === 'Enter' && participantSearch.trim()) { addParticipant(participantSearch.trim()); e.preventDefault() } }}
                        placeholder="Buscar o escribir nombre..."
                        className="w-full px-3 py-2 rounded-lg text-sm"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                      />
                      {participantSearch && (
                        <div className="absolute z-10 w-full mt-1 rounded-lg shadow-lg max-h-32 overflow-auto" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--border)' }}>
                          {usernames.filter(u => u.toLowerCase().includes(participantSearch.toLowerCase()) && !(form.participants || []).includes(u)).map(u => (
                            <button key={u} onClick={() => addParticipant(u)} className="w-full text-left px-3 py-2 text-sm hover:opacity-80" style={{ color: 'var(--text-primary)' }}>{u}</button>
                          ))}
                          {participantSearch.trim() && !usernames.includes(participantSearch.trim()) && (
                            <button onClick={() => addParticipant(participantSearch.trim())} className="w-full text-left px-3 py-2 text-sm hover:opacity-80" style={{ color: 'var(--accent)' }}>
                              + Agregar "{participantSearch.trim()}"
                            </button>
                          )}
                        </div>
                      )}
                    </div>
                  </div>
                </>
              )}

              {/* Seccion 3: Requisitos */}
              {modalSection === 3 && (
                <>
                  <div className="space-y-2">
                    {(form.requirements || []).map((req, i) => (
                      <div key={req.id} className="flex items-center gap-2">
                        <input
                          type="text"
                          value={req.text}
                          onChange={e => updateRequirement(i, e.target.value)}
                          placeholder="Descripcion del requisito..."
                          className="flex-1 px-3 py-2 rounded-lg text-sm"
                          style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                        />
                        <button onClick={() => removeRequirement(i)} style={{ color: 'var(--danger)' }}>
                          <Trash2 size={14} />
                        </button>
                      </div>
                    ))}
                  </div>
                  <button
                    onClick={addRequirement}
                    className="flex items-center gap-1.5 text-xs font-medium px-3 py-2 rounded-lg transition-all hover:opacity-80"
                    style={{ color: 'var(--accent)' }}
                  >
                    <Plus size={14} /> Agregar Requisito
                  </button>
                </>
              )}

              {/* Seccion 4: Hitos */}
              {modalSection === 4 && (
                <>
                  <div className="space-y-2">
                    {(form.milestones || []).map((mil, i) => (
                      <div key={mil.id} className="flex items-center gap-2">
                        <input
                          type="text"
                          value={mil.title}
                          onChange={e => updateMilestone(i, 'title', e.target.value)}
                          placeholder="Titulo del hito..."
                          className="flex-1 px-3 py-2 rounded-lg text-sm"
                          style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                        />
                        <input
                          type="date"
                          value={mil.date}
                          onChange={e => updateMilestone(i, 'date', e.target.value)}
                          className="w-36 px-3 py-2 rounded-lg text-sm"
                          style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                        />
                        <button onClick={() => removeMilestone(i)} style={{ color: 'var(--danger)' }}>
                          <Trash2 size={14} />
                        </button>
                      </div>
                    ))}
                  </div>
                  <button
                    onClick={addMilestone}
                    className="flex items-center gap-1.5 text-xs font-medium px-3 py-2 rounded-lg transition-all hover:opacity-80"
                    style={{ color: 'var(--accent)' }}
                  >
                    <Plus size={14} /> Agregar Hito
                  </button>
                </>
              )}

              {/* Seccion 5: Extras (tags, notas) */}
              {modalSection === 5 && (
                <>
                  {/* Tags */}
                  <div>
                    <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Tags</label>
                    <div className="flex flex-wrap gap-1.5 mb-2">
                      {(form.tags || []).map(tag => (
                        <span key={tag} className="text-xs px-2 py-1 rounded-full flex items-center gap-1" style={{ backgroundColor: 'var(--bg-secondary)', color: 'var(--text-secondary)' }}>
                          {tag}
                          <button onClick={() => removeTag(tag)}><X size={10} /></button>
                        </span>
                      ))}
                    </div>
                    <div className="flex gap-2">
                      <input
                        type="text"
                        value={tagInput}
                        onChange={e => setTagInput(e.target.value)}
                        onKeyDown={e => { if (e.key === 'Enter') { addTag(); e.preventDefault() } }}
                        placeholder="Nuevo tag..."
                        className="flex-1 px-3 py-2 rounded-lg text-sm"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                      />
                      <button
                        onClick={addTag}
                        className="px-3 py-2 rounded-lg text-sm"
                        style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}
                      >
                        <Plus size={14} />
                      </button>
                    </div>
                  </div>

                  {/* Notas */}
                  <div>
                    <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Notas</label>
                    <textarea
                      value={form.notes || ''}
                      onChange={e => setForm({ ...form, notes: e.target.value })}
                      rows={4}
                      placeholder="Notas adicionales..."
                      className="w-full px-3 py-2 rounded-lg text-sm resize-none"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                    />
                  </div>

                  {/* Archivos relacionados */}
                  <div>
                    <label className="text-xs font-medium mb-1 block" style={{ color: 'var(--text-secondary)' }}>Archivos Relacionados (rutas en NAS)</label>
                    <div className="space-y-1 mb-2">
                      {(form.related_files || []).map((f, i) => (
                        <div key={i} className="flex items-center gap-2">
                          <input
                            type="text"
                            value={f}
                            onChange={e => {
                              const files = [...(form.related_files || [])]
                              files[i] = e.target.value
                              setForm({ ...form, related_files: files })
                            }}
                            className="flex-1 px-3 py-2 rounded-lg text-sm"
                            style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                          />
                          <button onClick={() => {
                            const files = [...(form.related_files || [])]
                            files.splice(i, 1)
                            setForm({ ...form, related_files: files })
                          }} style={{ color: 'var(--danger)' }}>
                            <Trash2 size={14} />
                          </button>
                        </div>
                      ))}
                    </div>
                    <button
                      onClick={() => setForm({ ...form, related_files: [...(form.related_files || []), ''] })}
                      className="flex items-center gap-1.5 text-xs font-medium px-3 py-2 rounded-lg transition-all hover:opacity-80"
                      style={{ color: 'var(--accent)' }}
                    >
                      <Plus size={14} /> Agregar Ruta
                    </button>
                  </div>
                </>
              )}
            </div>

            {/* Footer modal */}
            <div className="flex items-center justify-end gap-3 px-6 py-4" style={{ borderTop: '1px solid var(--border)' }}>
              <button
                onClick={() => setShowModal(false)}
                className="px-4 py-2 rounded-lg text-sm font-medium transition-all hover:opacity-80"
                style={{ color: 'var(--text-secondary)' }}
              >
                Cancelar
              </button>
              <button
                onClick={handleSave}
                disabled={!form.title?.trim()}
                className="px-4 py-2 rounded-lg text-sm font-medium transition-all hover:opacity-90 disabled:opacity-50"
                style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
              >
                {editingEntry ? 'Guardar Cambios' : 'Crear Ficha'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}

function InfoField({ icon, label, value }: { icon: React.ReactNode; label: string; value: string }) {
  if (!value) return null
  return (
    <div>
      <label className="text-[11px] font-medium uppercase flex items-center gap-1" style={{ color: 'var(--text-secondary)' }}>
        {icon} {label}
      </label>
      <p className="text-sm mt-0.5" style={{ color: 'var(--text-primary)' }}>{value}</p>
    </div>
  )
}
