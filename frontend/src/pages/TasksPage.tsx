import { useEffect, useState, useCallback } from 'react'
import {
  Plus,
  Trash2,
  Loader2,
  X,
  CheckCircle2,
  XCircle,
  Clock,
  FolderOpen,
  ClipboardList,
  Bell,
  ShieldCheck,
  Calendar,
  Users,
} from 'lucide-react'
import {
  fetchProjects,
  createProject,
  deleteProject,
  fetchTasks,
  createTask,
  confirmTask,
  rejectTask,
  doneTask,
  deleteTask,
  fetchEvents,
  createEvent,
  deleteEvent,
  acceptEvent,
  declineEvent,
  fetchUsernames,
} from '../api'
import type { Task, Project, TaskStatus, CalendarEvent } from '../types'
import { useAuth } from '../auth/AuthContext'

const STATUS_CONFIG: Record<TaskStatus, { label: string; color: string; alpha: string }> = {
  pendiente: { label: 'Pendiente', color: 'var(--warning)', alpha: '20' },
  enprogreso: { label: 'En progreso', color: 'var(--accent)', alpha: '20' },
  completada: { label: 'Completada', color: 'var(--success)', alpha: '20' },
  rechazada: { label: 'Rechazada', color: 'var(--danger)', alpha: '20' },
}

const STATUS_ORDER: TaskStatus[] = ['pendiente', 'enprogreso', 'completada', 'rechazada']

export default function TasksPage() {
  const { user } = useAuth()
  const [tab, setTab] = useState<'tareas' | 'calendario'>('tareas')
  const [projects, setProjects] = useState<Project[]>([])
  const [tasks, setTasks] = useState<Task[]>([])
  const [loading, setLoading] = useState(true)
  const [selectedProject, setSelectedProject] = useState<string | null>(null)
  const [showNewProject, setShowNewProject] = useState(false)
  const [newProjectName, setNewProjectName] = useState('')
  const [showTaskModal, setShowTaskModal] = useState(false)

  // Calendario
  const [events, setEvents] = useState<CalendarEvent[]>([])
  const [showEventModal, setShowEventModal] = useState(false)
  const [eventTitle, setEventTitle] = useState('')
  const [eventDate, setEventDate] = useState('')
  const [eventTime, setEventTime] = useState('')
  const [eventInvitees, setEventInvitees] = useState('')
  const [eventRemind, setEventRemind] = useState(15)

  // Formulario de nueva tarea
  const [taskTitle, setTaskTitle] = useState('')
  const [taskProjectId, setTaskProjectId] = useState<string>('')
  const [taskAssignedTo, setTaskAssignedTo] = useState('')
  const [taskDueDate, setTaskDueDate] = useState('')
  const [taskRequiresConfirmation, setTaskRequiresConfirmation] = useState(false)
  const [taskInsistent, setTaskInsistent] = useState(false)
  const [taskReminderMinutes, setTaskReminderMinutes] = useState(8)
  const [allUsers, setAllUsers] = useState<string[]>([])

  const loadData = useCallback(async () => {
    setLoading(true)
    try {
      const [p, t, e, u] = await Promise.all([fetchProjects(), fetchTasks(), fetchEvents(), fetchUsernames().catch(() => [] as string[])])
      setProjects(p)
      setTasks(t)
      setEvents(e)
      setAllUsers(u)
    } catch {
      // silenciar
    } finally {
      setLoading(false)
    }
  }, [])

  useEffect(() => {
    loadData()
  }, [loadData])

  const filteredTasks = selectedProject
    ? tasks.filter((t) => t.project_id === selectedProject)
    : tasks

  // Estadisticas
  const totalTasks = filteredTasks.length
  const pendingTasks = filteredTasks.filter((t) => t.status === 'pendiente' || t.status === 'enprogreso').length
  const completedTasks = filteredTasks.filter((t) => t.status === 'completada').length
  const progress = totalTasks > 0 ? Math.round((completedTasks / totalTasks) * 100) : 0

  // Conteo de tareas por proyecto
  function projectTaskCount(projectId: string) {
    return tasks.filter((t) => t.project_id === projectId).length
  }
  function projectCompletedCount(projectId: string) {
    return tasks.filter((t) => t.project_id === projectId && t.status === 'completada').length
  }

  async function handleCreateProject() {
    if (!newProjectName.trim()) return
    try {
      await createProject({ name: newProjectName.trim() })
      setNewProjectName('')
      setShowNewProject(false)
      loadData()
    } catch { /* silenciar */ }
  }

  async function handleDeleteProject(id: string) {
    if (!confirm('Eliminar este proyecto?')) return
    try {
      await deleteProject(id)
      if (selectedProject === id) setSelectedProject(null)
      loadData()
    } catch { /* silenciar */ }
  }

  function openTaskModal() {
    setTaskTitle('')
    setTaskProjectId(selectedProject || '')
    setTaskAssignedTo('')
    setTaskDueDate('')
    setTaskRequiresConfirmation(false)
    setTaskInsistent(false)
    setTaskReminderMinutes(8)
    setShowTaskModal(true)
  }

  async function handleCreateTask() {
    if (!taskTitle.trim()) return
    const assigned = taskAssignedTo.trim()
      ? taskAssignedTo.split(',').map((s) => s.trim()).filter(Boolean)
      : []

    try {
      await createTask({
        title: taskTitle.trim(),
        project_id: taskProjectId || null,
        assigned_to: assigned,
        requires_confirmation: taskRequiresConfirmation,
        insistent: taskInsistent,
        reminder_minutes: taskReminderMinutes,
        due_date: taskDueDate || null,
      })
      setShowTaskModal(false)
      loadData()
    } catch { /* silenciar */ }
  }

  async function handleConfirm(id: string) {
    try {
      await confirmTask(id, user?.username || 'web')
      loadData()
    } catch { /* silenciar */ }
  }

  async function handleReject(id: string) {
    try {
      await rejectTask(id, user?.username || 'web')
      loadData()
    } catch { /* silenciar */ }
  }

  async function handleDone(id: string) {
    try {
      await doneTask(id)
      loadData()
    } catch { /* silenciar */ }
  }

  async function handleDelete(id: string) {
    if (!confirm('Eliminar esta tarea?')) return
    try {
      await deleteTask(id)
      loadData()
    } catch { /* silenciar */ }
  }

  if (loading) {
    return (
      <div className="flex flex-col items-center justify-center py-16 gap-3">
        <Loader2 size={32} className="animate-spin" style={{ color: 'var(--accent)' }} />
        <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>Cargando tareas...</p>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      {/* Tabs */}
      <div className="flex gap-1 p-1 rounded-lg w-fit" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
        <button
          onClick={() => setTab('tareas')}
          className="px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200"
          style={{
            backgroundColor: tab === 'tareas' ? 'var(--card-bg)' : 'transparent',
            color: tab === 'tareas' ? 'var(--accent)' : 'var(--text-secondary)',
            boxShadow: tab === 'tareas' ? '0 1px 3px rgba(0,0,0,0.1)' : 'none',
          }}
        >
          <span className="flex items-center gap-2"><ClipboardList size={16} />Tareas</span>
        </button>
        <button
          onClick={() => setTab('calendario')}
          className="px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200"
          style={{
            backgroundColor: tab === 'calendario' ? 'var(--card-bg)' : 'transparent',
            color: tab === 'calendario' ? 'var(--accent)' : 'var(--text-secondary)',
            boxShadow: tab === 'calendario' ? '0 1px 3px rgba(0,0,0,0.1)' : 'none',
          }}
        >
          <span className="flex items-center gap-2"><Calendar size={16} />Calendario</span>
        </button>
      </div>

      {/* Calendario */}
      {tab === 'calendario' && (
        <div className="space-y-4">
          <div className="flex items-center justify-between">
            <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
              {events.length} evento{events.length !== 1 ? 's' : ''}
            </span>
            <button
              onClick={() => { setShowEventModal(true); setEventTitle(''); setEventDate(''); setEventTime(''); setEventInvitees(''); setEventRemind(15) }}
              className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              <Plus size={16} />Nuevo Evento
            </button>
          </div>

          {events.length === 0 ? (
            <div className="rounded-xl p-12 text-center" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
              <Calendar size={40} className="mx-auto mb-3" style={{ color: 'var(--text-secondary)' }} />
              <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>No hay eventos</p>
            </div>
          ) : (
            <div className="space-y-3">
              {events
                .slice()
                .sort((a, b) => `${a.date} ${a.time}`.localeCompare(`${b.date} ${b.time}`))
                .map(ev => {
                  const isPast = `${ev.date} ${ev.time}` < new Date().toISOString().slice(0, 16).replace('T', ' ')
                  return (
                    <div
                      key={ev.id}
                      className="rounded-xl p-5"
                      style={{
                        backgroundColor: 'var(--card-bg)',
                        border: '1px solid var(--card-border)',
                        opacity: isPast ? 0.5 : 1,
                      }}
                    >
                      <div className="flex items-start justify-between">
                        <div>
                          <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>{ev.title}</h3>
                          <p className="text-xs mt-1 font-mono" style={{ color: 'var(--accent)' }}>
                            {ev.date} {ev.time}
                          </p>
                          <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
                            Por: {ev.created_by} | Aviso: {ev.remind_before_min}min antes
                          </p>
                          {ev.invitees.length > 0 && (
                            <p className="text-xs mt-1" style={{ color: 'var(--text-secondary)' }}>
                              Invitados: {ev.invitees.join(', ')}
                            </p>
                          )}
                          {(ev.accepted.length > 0 || ev.declined.length > 0) && (
                            <div className="flex gap-3 mt-2 text-xs">
                              {ev.accepted.length > 0 && (
                                <span style={{ color: 'var(--success)' }}>Aceptaron: {ev.accepted.join(', ')}</span>
                              )}
                              {ev.declined.length > 0 && (
                                <span style={{ color: 'var(--danger)' }}>Rechazaron: {ev.declined.join(', ')}</span>
                              )}
                            </div>
                          )}
                        </div>
                        <div className="flex items-center gap-1.5">
                          {user && !ev.accepted.includes(user.username) && !ev.declined.includes(user.username) && ev.created_by !== user.username && (
                            <>
                              <button
                                onClick={async () => { await acceptEvent(ev.id, user!.username); await loadData() }}
                                className="px-2 py-1 rounded-lg text-xs font-medium"
                                style={{ color: 'var(--success)', border: '1px solid var(--success)' }}
                              >Aceptar</button>
                              <button
                                onClick={async () => { await declineEvent(ev.id, user!.username); await loadData() }}
                                className="px-2 py-1 rounded-lg text-xs font-medium"
                                style={{ color: 'var(--danger)', border: '1px solid var(--danger)' }}
                              >Rechazar</button>
                            </>
                          )}
                          <button
                            onClick={async () => { await deleteEvent(ev.id); await loadData() }}
                            className="p-1.5 rounded-lg hover:opacity-80"
                            style={{ color: 'var(--danger)' }}
                          ><Trash2 size={14} /></button>
                        </div>
                      </div>
                    </div>
                  )
                })}
            </div>
          )}

          {/* Event Modal */}
          {showEventModal && (
            <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
              <div className="rounded-xl p-6 w-full max-w-md mx-4" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}>
                <div className="flex items-center justify-between mb-4">
                  <h3 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Nuevo Evento</h3>
                  <button onClick={() => setShowEventModal(false)} style={{ color: 'var(--text-secondary)' }}><X size={20} /></button>
                </div>
                <div className="space-y-3">
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Titulo</label>
                    <input value={eventTitle} onChange={e => setEventTitle(e.target.value)} placeholder="Reunion de equipo"
                      className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                  </div>
                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Fecha</label>
                      <input type="date" value={eventDate} onChange={e => setEventDate(e.target.value)}
                        className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                    </div>
                    <div>
                      <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Hora</label>
                      <input type="time" value={eventTime} onChange={e => setEventTime(e.target.value)}
                        className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                    </div>
                  </div>
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Invitados (separar con comas, o "all")</label>
                    <input value={eventInvitees} onChange={e => setEventInvitees(e.target.value)} placeholder="nick, ana, all"
                      className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                  </div>
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Avisar (minutos antes)</label>
                    <input type="number" min={1} value={eventRemind} onChange={e => setEventRemind(parseInt(e.target.value) || 15)}
                      className="w-24 px-3 py-2 rounded-lg text-sm outline-none"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                  </div>
                </div>
                <div className="flex justify-end gap-3 mt-5">
                  <button onClick={() => setShowEventModal(false)} className="px-4 py-2 rounded-lg text-sm font-medium"
                    style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>Cancelar</button>
                  <button
                    onClick={async () => {
                      if (!eventTitle.trim() || !eventDate || !eventTime) return
                      const invitees = eventInvitees.split(',').map(s => s.trim()).filter(Boolean)
                      await createEvent({ title: eventTitle, date: eventDate, time: eventTime, invitees, remind_before_min: eventRemind })
                      setShowEventModal(false)
                      await loadData()
                    }}
                    disabled={!eventTitle.trim() || !eventDate || !eventTime}
                    className="px-4 py-2 rounded-lg text-sm font-medium"
                    style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
                  >Crear</button>
                </div>
              </div>
            </div>
          )}
        </div>
      )}

      {/* Tareas */}
      {tab === 'tareas' && (
    <div className="flex gap-6 h-full" style={{ minHeight: 0 }}>
      {/* Sidebar de proyectos */}
      <div
        className="w-[260px] min-w-[260px] flex flex-col rounded-xl overflow-hidden"
        style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
      >
        <div className="px-4 py-3 flex items-center justify-between" style={{ borderBottom: '1px solid var(--border)' }}>
          <span className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Proyectos</span>
          <button
            onClick={() => setShowNewProject(!showNewProject)}
            className="p-1 rounded-lg transition-all duration-200 hover:opacity-80"
            style={{ color: 'var(--accent)' }}
            title="Nuevo proyecto"
          >
            <Plus size={18} />
          </button>
        </div>

        {/* Input nuevo proyecto */}
        {showNewProject && (
          <div className="px-3 py-2" style={{ borderBottom: '1px solid var(--border)' }}>
            <input
              type="text"
              value={newProjectName}
              onChange={(e) => setNewProjectName(e.target.value)}
              onKeyDown={(e) => e.key === 'Enter' && handleCreateProject()}
              placeholder="Nombre del proyecto"
              className="w-full px-3 py-1.5 rounded-lg text-sm outline-none mb-2"
              style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
              autoFocus
            />
            <div className="flex gap-2">
              <button
                onClick={handleCreateProject}
                disabled={!newProjectName.trim()}
                className="flex-1 px-2 py-1 rounded-lg text-xs font-medium"
                style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
              >
                Crear
              </button>
              <button
                onClick={() => { setShowNewProject(false); setNewProjectName('') }}
                className="px-2 py-1 rounded-lg text-xs font-medium"
                style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
              >
                Cancelar
              </button>
            </div>
          </div>
        )}

        {/* Lista de proyectos */}
        <div className="flex-1 overflow-auto">
          {/* Opcion: Todas */}
          <button
            onClick={() => setSelectedProject(null)}
            className="w-full text-left px-4 py-2.5 flex items-center gap-3 transition-all duration-200"
            style={{
              backgroundColor: selectedProject === null ? 'var(--accent-alpha)' : 'transparent',
              color: selectedProject === null ? 'var(--accent)' : 'var(--text-secondary)',
            }}
          >
            <ClipboardList size={16} />
            <div className="flex-1 min-w-0">
              <span className="text-sm font-medium">Todas</span>
              <span className="text-xs ml-2" style={{ color: 'var(--text-secondary)' }}>
                ({tasks.length})
              </span>
            </div>
          </button>

          {projects.map((project) => {
            const total = projectTaskCount(project.id)
            const completed = projectCompletedCount(project.id)
            const pct = total > 0 ? Math.round((completed / total) * 100) : 0
            const isSelected = selectedProject === project.id

            return (
              <div
                key={project.id}
                className="group relative"
                style={{
                  backgroundColor: isSelected ? 'var(--accent-alpha)' : 'transparent',
                }}
              >
                <button
                  onClick={() => setSelectedProject(project.id)}
                  className="w-full text-left px-4 py-2.5 flex items-center gap-3 transition-all duration-200"
                  style={{
                    color: isSelected ? 'var(--accent)' : 'var(--text-secondary)',
                  }}
                >
                  <FolderOpen size={16} />
                  <div className="flex-1 min-w-0">
                    <div className="flex items-center gap-2">
                      <span className="text-sm font-medium truncate">{project.name}</span>
                      <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                        ({total})
                      </span>
                    </div>
                    {total > 0 && (
                      <div className="mt-1 h-1 rounded-full overflow-hidden" style={{ backgroundColor: 'var(--border)' }}>
                        <div
                          className="h-full rounded-full transition-all duration-300"
                          style={{ width: `${pct}%`, backgroundColor: 'var(--success)' }}
                        />
                      </div>
                    )}
                  </div>
                </button>
                <button
                  onClick={(e) => { e.stopPropagation(); handleDeleteProject(project.id) }}
                  className="absolute right-2 top-1/2 -translate-y-1/2 p-1 rounded opacity-0 group-hover:opacity-100 transition-opacity"
                  style={{ color: 'var(--danger)' }}
                  title="Eliminar proyecto"
                >
                  <Trash2 size={14} />
                </button>
              </div>
            )
          })}

          {projects.length === 0 && (
            <div className="text-center py-8 px-4">
              <FolderOpen size={24} className="mx-auto mb-2" style={{ color: 'var(--text-secondary)' }} />
              <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                Sin proyectos. Crea uno para organizar tus tareas.
              </p>
            </div>
          )}
        </div>
      </div>

      {/* Panel principal de tareas */}
      <div className="flex-1 flex flex-col min-w-0 gap-4">
        {/* Estadisticas + Boton nueva tarea */}
        <div className="flex items-center justify-between flex-wrap gap-4">
          <div className="flex items-center gap-6">
            <div className="flex items-center gap-2">
              <ClipboardList size={18} style={{ color: 'var(--text-secondary)' }} />
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                Total: <strong style={{ color: 'var(--text-primary)' }}>{totalTasks}</strong>
              </span>
            </div>
            <div className="flex items-center gap-2">
              <Clock size={18} style={{ color: 'var(--warning)' }} />
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                Pendientes: <strong style={{ color: 'var(--warning)' }}>{pendingTasks}</strong>
              </span>
            </div>
            <div className="flex items-center gap-2">
              <CheckCircle2 size={18} style={{ color: 'var(--success)' }} />
              <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                Completadas: <strong style={{ color: 'var(--success)' }}>{completedTasks}</strong>
              </span>
            </div>
            <div className="flex items-center gap-2">
              <span className="text-sm font-medium" style={{ color: 'var(--accent)' }}>
                {progress}%
              </span>
            </div>
          </div>

          <button
            onClick={openTaskModal}
            className="flex items-center gap-2 px-5 py-2.5 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
            style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
          >
            <Plus size={18} />
            Nueva Tarea
          </button>
        </div>

        {/* Lista de tareas agrupadas por estado */}
        <div className="flex-1 overflow-auto space-y-6">
          {filteredTasks.length === 0 ? (
            <div
              className="rounded-xl text-center py-16"
              style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
            >
              <ClipboardList size={48} className="mx-auto mb-4" style={{ color: 'var(--text-secondary)' }} />
              <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                No hay tareas. Crea una con el boton "Nueva Tarea".
              </p>
            </div>
          ) : (
            STATUS_ORDER.map((status) => {
              const group = filteredTasks.filter((t) => t.status === status)
              if (group.length === 0) return null
              const cfg = STATUS_CONFIG[status]

              return (
                <div key={status}>
                  <div className="flex items-center gap-2 mb-3">
                    <span
                      className="inline-flex items-center gap-1.5 px-2.5 py-1 rounded-full text-xs font-semibold"
                      style={{ backgroundColor: cfg.color + cfg.alpha, color: cfg.color }}
                    >
                      {cfg.label}
                    </span>
                    <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                      {group.length}
                    </span>
                  </div>

                  <div className="grid gap-3">
                    {group.map((task) => (
                      <TaskCard
                        key={task.id}
                        task={task}
                        projects={projects}
                        onConfirm={() => handleConfirm(task.id)}
                        onReject={() => handleReject(task.id)}
                        onDone={() => handleDone(task.id)}
                        onDelete={() => handleDelete(task.id)}
                      />
                    ))}
                  </div>
                </div>
              )
            })
          )}
        </div>
      </div>

      {/* Modal nueva tarea */}
      {showTaskModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div
            className="rounded-xl p-6 w-full max-w-md mx-4"
            style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
          >
            <div className="flex items-center justify-between mb-5">
              <h3 className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>Nueva Tarea</h3>
              <button onClick={() => setShowTaskModal(false)} style={{ color: 'var(--text-secondary)' }}>
                <X size={18} />
              </button>
            </div>

            <div className="space-y-4">
              {/* Titulo */}
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Titulo</label>
                <input
                  type="text"
                  value={taskTitle}
                  onChange={(e) => setTaskTitle(e.target.value)}
                  placeholder="Descripcion de la tarea"
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                  autoFocus
                />
              </div>

              {/* Proyecto */}
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Proyecto (opcional)</label>
                <select
                  value={taskProjectId}
                  onChange={(e) => setTaskProjectId(e.target.value)}
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                >
                  <option value="">Sin proyecto</option>
                  {projects.map((p) => (
                    <option key={p.id} value={p.id}>{p.name}</option>
                  ))}
                </select>
              </div>

              {/* Asignar a */}
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Asignar a</label>
                <div className="flex flex-wrap gap-1.5 mb-2">
                  <button
                    type="button"
                    onClick={() => setTaskAssignedTo('all')}
                    className="px-2.5 py-1 rounded-full text-xs font-medium transition-all"
                    style={{
                      backgroundColor: taskAssignedTo === 'all' ? 'var(--accent)' : 'var(--bg-tertiary)',
                      color: taskAssignedTo === 'all' ? '#ffffff' : 'var(--text-secondary)',
                      border: `1px solid ${taskAssignedTo === 'all' ? 'var(--accent)' : 'var(--border)'}`,
                    }}
                  >
                    @todos
                  </button>
                  {allUsers.map((u) => {
                    const selected = taskAssignedTo.split(',').map(s => s.trim()).includes(u)
                    return (
                      <button
                        key={u}
                        type="button"
                        onClick={() => {
                          const current = taskAssignedTo.split(',').map(s => s.trim()).filter(Boolean)
                          if (current.includes('all')) {
                            setTaskAssignedTo(u)
                          } else if (selected) {
                            setTaskAssignedTo(current.filter(x => x !== u).join(', '))
                          } else {
                            setTaskAssignedTo([...current, u].join(', '))
                          }
                        }}
                        className="px-2.5 py-1 rounded-full text-xs font-medium transition-all"
                        style={{
                          backgroundColor: selected && taskAssignedTo !== 'all' ? 'var(--accent)' : 'var(--bg-tertiary)',
                          color: selected && taskAssignedTo !== 'all' ? '#ffffff' : 'var(--text-secondary)',
                          border: `1px solid ${selected && taskAssignedTo !== 'all' ? 'var(--accent)' : 'var(--border)'}`,
                        }}
                      >
                        @{u}
                      </button>
                    )
                  })}
                </div>
                <input
                  type="text"
                  value={taskAssignedTo}
                  onChange={(e) => setTaskAssignedTo(e.target.value)}
                  placeholder="O escribe nombres separados por comas"
                  className="w-full px-3 py-2 rounded-lg text-xs outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                />
              </div>

              {/* Fecha limite */}
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Fecha limite (opcional)</label>
                <input
                  type="date"
                  value={taskDueDate}
                  onChange={(e) => setTaskDueDate(e.target.value)}
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                />
              </div>

              {/* Checkboxes */}
              <div className="flex items-center gap-6">
                <label className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={taskRequiresConfirmation}
                    onChange={(e) => setTaskRequiresConfirmation(e.target.checked)}
                    className="accent-current"
                    style={{ accentColor: 'var(--accent)' }}
                  />
                  <span className="text-sm" style={{ color: 'var(--text-primary)' }}>Requiere confirmacion</span>
                </label>
                <label className="flex items-center gap-2 cursor-pointer">
                  <input
                    type="checkbox"
                    checked={taskInsistent}
                    onChange={(e) => setTaskInsistent(e.target.checked)}
                    className="accent-current"
                    style={{ accentColor: 'var(--accent)' }}
                  />
                  <span className="text-sm" style={{ color: 'var(--text-primary)' }}>Insistente</span>
                </label>
              </div>

              {/* Minutos recordatorio */}
              {taskInsistent && (
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Recordar cada (minutos)</label>
                  <input
                    type="number"
                    min={1}
                    value={taskReminderMinutes}
                    onChange={(e) => setTaskReminderMinutes(parseInt(e.target.value) || 8)}
                    className="w-24 px-3 py-2 rounded-lg text-sm outline-none"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                  />
                </div>
              )}
            </div>

            <div className="flex items-center justify-end gap-3 mt-6">
              <button
                onClick={() => setShowTaskModal(false)}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
              >
                Cancelar
              </button>
              <button
                onClick={handleCreateTask}
                disabled={!taskTitle.trim()}
                className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
                style={{ backgroundColor: 'var(--accent)', color: '#ffffff', opacity: taskTitle.trim() ? 1 : 0.5 }}
              >
                <Plus size={14} />
                Crear Tarea
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
      )}
    </div>
  )
}

// Componente de tarjeta de tarea
function TaskCard({
  task,
  projects,
  onConfirm,
  onReject,
  onDone,
  onDelete,
}: {
  task: Task
  projects: Project[]
  onConfirm: () => void
  onReject: () => void
  onDone: () => void
  onDelete: () => void
}) {
  const projectName = task.project_id
    ? projects.find((p) => p.id === task.project_id)?.name || 'Sin proyecto'
    : null

  const isActive = task.status === 'pendiente' || task.status === 'enprogreso'

  return (
    <div
      className="rounded-xl p-4 transition-all duration-200"
      style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
    >
      <div className="flex items-start justify-between gap-3">
        <div className="flex-1 min-w-0">
          {/* Titulo y badges */}
          <div className="flex items-center gap-2 flex-wrap mb-1">
            <h4 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>
              {task.title}
            </h4>
            {task.requires_confirmation && (
              <span
                className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium"
                style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}
              >
                <ShieldCheck size={10} />
                Confirmacion
              </span>
            )}
            {task.insistent && (
              <span
                className="inline-flex items-center gap-1 px-1.5 py-0.5 rounded text-[10px] font-medium"
                style={{ backgroundColor: 'var(--warning)' + '20', color: 'var(--warning)' }}
              >
                <Bell size={10} />
                Insistente ({task.reminder_minutes}min)
              </span>
            )}
          </div>

          {/* Metadatos */}
          <div className="flex items-center gap-3 flex-wrap text-xs" style={{ color: 'var(--text-secondary)' }}>
            {projectName && (
              <span className="flex items-center gap-1">
                <FolderOpen size={12} />
                {projectName}
              </span>
            )}
            {task.assigned_to.length > 0 && (
              <span className="flex items-center gap-1">
                <Users size={12} />
                {task.assigned_to.join(', ')}
              </span>
            )}
            {task.due_date && (
              <span className="flex items-center gap-1">
                <Calendar size={12} />
                {task.due_date}
              </span>
            )}
            <span>
              por {task.created_by}
            </span>
          </div>

          {/* Confirmaciones / Rechazos */}
          {(task.confirmed_by.length > 0 || task.rejected_by.length > 0) && (
            <div className="flex items-center gap-3 mt-2 text-xs">
              {task.confirmed_by.length > 0 && (
                <span className="flex items-center gap-1" style={{ color: 'var(--success)' }}>
                  <CheckCircle2 size={12} />
                  Confirmado: {task.confirmed_by.join(', ')}
                </span>
              )}
              {task.rejected_by.length > 0 && (
                <span className="flex items-center gap-1" style={{ color: 'var(--danger)' }}>
                  <XCircle size={12} />
                  Rechazado: {task.rejected_by.join(', ')}
                </span>
              )}
            </div>
          )}
        </div>

        {/* Acciones */}
        <div className="flex items-center gap-1.5 shrink-0">
          {isActive && task.requires_confirmation && (
            <>
              <button
                onClick={onConfirm}
                className="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-80"
                style={{ color: 'var(--success)', border: '1px solid var(--success)' }}
                title="Confirmar"
              >
                <CheckCircle2 size={12} />
                Confirmar
              </button>
              <button
                onClick={onReject}
                className="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-80"
                style={{ color: 'var(--danger)', border: '1px solid var(--danger)' }}
                title="Rechazar"
              >
                <XCircle size={12} />
                Rechazar
              </button>
            </>
          )}
          {isActive && (
            <button
              onClick={onDone}
              className="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-80"
              style={{ color: 'var(--success)', border: '1px solid var(--success)' }}
              title="Marcar como completada"
            >
              <CheckCircle2 size={12} />
              Hecho
            </button>
          )}
          <button
            onClick={onDelete}
            className="inline-flex items-center gap-1 px-2 py-1 rounded-lg text-xs font-medium transition-all duration-200 hover:opacity-80"
            style={{ color: 'var(--danger)', border: '1px solid var(--border)' }}
            title="Eliminar"
          >
            <Trash2 size={12} />
          </button>
        </div>
      </div>
    </div>
  )
}
