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
  Tag,
  UserPlus,
  ChevronLeft,
  ChevronRight,
  Pencil,
  MapPin,
  BellOff,
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
  scheduleTask,
  updateProject,
  fetchCategories,
  createCategory,
  deleteCategory,
  updateEvent,
  updateCategory,
} from '../api'
import type { Task, Project, TaskStatus, CalendarEvent, EventCategory } from '../types'
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
  const [eventInvitees, setEventInvitees] = useState<string[]>([])
  const [inviteeSearch, setInviteeSearch] = useState('')
  const [eventEndTime, setEventEndTime] = useState('')
  const [eventDescription, setEventDescription] = useState('')
  const [eventLocation, setEventLocation] = useState('')
  const [eventRemind, setEventRemind] = useState(15)
  const [eventNotifyTg, setEventNotifyTg] = useState(true)
  const [eventRecurrence, setEventRecurrence] = useState('')
  const [eventRecurrenceEnd, setEventRecurrenceEnd] = useState('')
  const [editingEventId, setEditingEventId] = useState<string | null>(null)
  const [calendarMonth, setCalendarMonth] = useState(() => { const d = new Date(); return new Date(d.getFullYear(), d.getMonth(), 1) })
  const [selectedDay, setSelectedDay] = useState<string | null>(null)
  const [calendarView, setCalendarView] = useState<'month' | 'week'>('month')
  const [categories, setCategories] = useState<EventCategory[]>([])
  const [eventCategory, setEventCategory] = useState('')
  const [showCategoryManager, setShowCategoryManager] = useState(false)
  const [newCatName, setNewCatName] = useState('')
  const [newCatColor, setNewCatColor] = useState('#7aa2f7')

  // Formulario de nueva tarea
  const [taskTitle, setTaskTitle] = useState('')
  const [taskProjectId, setTaskProjectId] = useState<string>('')
  const [taskAssignedTo, setTaskAssignedTo] = useState('')
  const [taskDueDate, setTaskDueDate] = useState('')
  const [taskRequiresConfirmation, setTaskRequiresConfirmation] = useState(false)
  const [taskInsistent, setTaskInsistent] = useState(false)
  const [taskReminderMinutes, setTaskReminderMinutes] = useState(8)
  const [taskDueTime, setTaskDueTime] = useState('')
  const [allUsers, setAllUsers] = useState<string[]>([])

  // Panel de miembros/tags del proyecto
  const [showMembers, setShowMembers] = useState(false)
  const [newMember, setNewMember] = useState('')
  const [newTagUser, setNewTagUser] = useState('')
  const [newTagValue, setNewTagValue] = useState('')

  // Modal de agendar tarea (cuando falta fecha/hora al confirmar)
  const [showScheduleModal, setShowScheduleModal] = useState(false)
  const [scheduleTaskId, setScheduleTaskId] = useState('')
  const [scheduleDate, setScheduleDate] = useState('')
  const [scheduleTime, setScheduleTime] = useState('')
  const [scheduleMissing, setScheduleMissing] = useState<'date' | 'time' | 'both'>('both')

  const loadData = useCallback(async () => {
    setLoading(true)
    try {
      const [p, t, e, u, cats] = await Promise.all([fetchProjects(), fetchTasks(), fetchEvents(), fetchUsernames().catch(() => [] as string[]), fetchCategories().catch(() => [] as EventCategory[])])
      setProjects(p)
      setTasks(t)
      setEvents(e)
      setAllUsers(u)
      setCategories(cats)
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
    setTaskDueTime('')
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
        due_time: taskDueTime || null,
      })
      setShowTaskModal(false)
      loadData()
    } catch { /* silenciar */ }
  }

  async function handleConfirm(id: string) {
    try {
      const task = tasks.find((t) => t.id === id)
      const hasDate = !!task?.due_date
      const hasTime = !!task?.due_time

      await confirmTask(id, user?.username || 'web')

      // Si tiene fecha y hora, el backend ya creo el evento
      if (hasDate && hasTime) {
        loadData()
        return
      }

      // Si falta fecha, hora o ambos, abrir modal para completar
      setScheduleTaskId(id)
      setScheduleDate(task?.due_date || '')
      setScheduleTime(task?.due_time || '')
      setScheduleMissing(!hasDate && !hasTime ? 'both' : !hasDate ? 'date' : 'time')
      setShowScheduleModal(true)
      loadData()
    } catch { /* silenciar */ }
  }

  async function handleAddMember(projectId: string) {
    if (!newMember.trim()) return
    const project = projects.find((p) => p.id === projectId)
    if (!project) return
    const members = [...project.members, newMember.trim()]
    try {
      await updateProject(projectId, { members })
      setNewMember('')
      loadData()
    } catch { /* silenciar */ }
  }

  async function handleRemoveMember(projectId: string, member: string) {
    const project = projects.find((p) => p.id === projectId)
    if (!project) return
    const members = project.members.filter((m) => m !== member)
    const memberTags = { ...project.member_tags }
    delete memberTags[member]
    try {
      await updateProject(projectId, { members, member_tags: memberTags })
      loadData()
    } catch { /* silenciar */ }
  }

  async function handleAddTag(projectId: string, username: string) {
    if (!newTagValue.trim()) return
    const project = projects.find((p) => p.id === projectId)
    if (!project) return
    const memberTags = { ...project.member_tags }
    const tags = memberTags[username] || []
    if (!tags.includes(newTagValue.trim())) {
      memberTags[username] = [...tags, newTagValue.trim()]
    }
    try {
      await updateProject(projectId, { member_tags: memberTags })
      setNewTagValue('')
      setNewTagUser('')
      loadData()
    } catch { /* silenciar */ }
  }

  async function handleRemoveTag(projectId: string, username: string, tag: string) {
    const project = projects.find((p) => p.id === projectId)
    if (!project) return
    const memberTags = { ...project.member_tags }
    memberTags[username] = (memberTags[username] || []).filter((t) => t !== tag)
    if (memberTags[username].length === 0) delete memberTags[username]
    try {
      await updateProject(projectId, { member_tags: memberTags })
      loadData()
    } catch { /* silenciar */ }
  }

  async function handleScheduleConfirm() {
    if (!scheduleDate || !scheduleTime) return
    try {
      await scheduleTask(scheduleTaskId, { date: scheduleDate, time: scheduleTime })
      setShowScheduleModal(false)
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
      {tab === 'calendario' && (() => {
        const today = new Date()
        const todayStr = today.toISOString().slice(0, 10)
        const year = calendarMonth.getFullYear()
        const month = calendarMonth.getMonth()
        const firstDay = new Date(year, month, 1)
        const lastDay = new Date(year, month + 1, 0)
        const startPad = (firstDay.getDay() + 6) % 7 // Lunes = 0
        const daysInMonth = lastDay.getDate()
        const prevMonth = new Date(year, month - 1, 1)
        const prevMonthLastDay = new Date(year, month, 0).getDate()

        // Build calendar grid (6 weeks max)
        const cells: { day: number; month: number; year: number; dateStr: string; isCurrentMonth: boolean }[] = []
        // Previous month padding
        for (let i = startPad - 1; i >= 0; i--) {
          const d = prevMonthLastDay - i
          const m = prevMonth.getMonth()
          const y = prevMonth.getFullYear()
          cells.push({ day: d, month: m, year: y, dateStr: `${y}-${String(m + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`, isCurrentMonth: false })
        }
        // Current month
        for (let d = 1; d <= daysInMonth; d++) {
          cells.push({ day: d, month, year, dateStr: `${year}-${String(month + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`, isCurrentMonth: true })
        }
        // Next month padding
        const remaining = 7 - (cells.length % 7)
        if (remaining < 7) {
          const nm = month + 1
          const ny = nm > 11 ? year + 1 : year
          const nmNorm = nm % 12
          for (let d = 1; d <= remaining; d++) {
            cells.push({ day: d, month: nmNorm, year: ny, dateStr: `${ny}-${String(nmNorm + 1).padStart(2, '0')}-${String(d).padStart(2, '0')}`, isCurrentMonth: false })
          }
        }

        // Map events by date
        const eventsByDate: Record<string, CalendarEvent[]> = {}
        for (const ev of events) {
          if (!eventsByDate[ev.date]) eventsByDate[ev.date] = []
          eventsByDate[ev.date].push(ev)
        }
        // Also map tasks with due_date
        const tasksByDate: Record<string, Task[]> = {}
        for (const t of tasks) {
          if (t.due_date) {
            if (!tasksByDate[t.due_date]) tasksByDate[t.due_date] = []
            tasksByDate[t.due_date].push(t)
          }
        }

        // Category color helper
        const catMap = new Map(categories.map(c => [c.id, c]))
        const getCatColor = (ev: CalendarEvent) => ev.category ? catMap.get(ev.category)?.color : undefined

        const monthNames = ['Enero', 'Febrero', 'Marzo', 'Abril', 'Mayo', 'Junio', 'Julio', 'Agosto', 'Septiembre', 'Octubre', 'Noviembre', 'Diciembre']
        const dayNames = ['Lun', 'Mar', 'Mie', 'Jue', 'Vie', 'Sab', 'Dom']

        const selectedEvents = selectedDay ? (eventsByDate[selectedDay] || []).sort((a, b) => a.time.localeCompare(b.time)) : []
        const selectedTasks = selectedDay ? (tasksByDate[selectedDay] || []) : []

        return (
        <div className="space-y-4">
          {/* Header */}
          <div className="flex items-center justify-between">
            <div className="flex items-center gap-3">
              <button onClick={() => setCalendarMonth(new Date(year, month - 1, 1))} className="p-1.5 rounded-lg hover:opacity-80" style={{ color: 'var(--text-secondary)' }}>
                <ChevronLeft size={18} />
              </button>
              <h2 className="text-base font-semibold min-w-[180px] text-center" style={{ color: 'var(--text-primary)' }}>
                {monthNames[month]} {year}
              </h2>
              <button onClick={() => setCalendarMonth(new Date(year, month + 1, 1))} className="p-1.5 rounded-lg hover:opacity-80" style={{ color: 'var(--text-secondary)' }}>
                <ChevronRight size={18} />
              </button>
              <button
                onClick={() => { setCalendarMonth(new Date(today.getFullYear(), today.getMonth(), 1)); setSelectedDay(todayStr) }}
                className="text-xs px-2.5 py-1 rounded-lg font-medium"
                style={{ color: 'var(--accent)', border: '1px solid var(--border)' }}
              >Hoy</button>
            </div>
            <button
              onClick={() => {
                setShowEventModal(true); setEventTitle(''); setEventTime('')
                setEventDate(selectedDay || todayStr)
                setEventInvitees([]); setInviteeSearch(''); setEventEndTime(''); setEventDescription(''); setEventLocation('')
                setEventRemind(15); setEventNotifyTg(true); setEventRecurrence(''); setEventRecurrenceEnd(''); setEventCategory(''); setEditingEventId(null)
              }}
              className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              <Plus size={16} />Nuevo Evento
            </button>
          </div>

          <div className="grid grid-cols-1 lg:grid-cols-[1fr_320px] gap-4">
            {/* Calendar Grid */}
            <div className="rounded-xl overflow-hidden" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
              {/* Day headers */}
              <div className="grid grid-cols-7">
                {dayNames.map(d => (
                  <div key={d} className="py-2 text-center text-[10px] font-semibold uppercase tracking-wider" style={{ color: 'var(--text-secondary)', borderBottom: '1px solid var(--border)' }}>
                    {d}
                  </div>
                ))}
              </div>
              {/* Day cells */}
              <div className="grid grid-cols-7">
                {cells.map((cell, i) => {
                  const isToday = cell.dateStr === todayStr
                  const isSelected = cell.dateStr === selectedDay
                  const dayEvents = eventsByDate[cell.dateStr] || []
                  const dayTasks = tasksByDate[cell.dateStr] || []
                  const hasItems = dayEvents.length > 0 || dayTasks.length > 0
                  return (
                    <div
                      key={i}
                      onClick={() => setSelectedDay(cell.dateStr === selectedDay ? null : cell.dateStr)}
                      className="relative p-1.5 min-h-[80px] cursor-pointer transition-all duration-150 hover:opacity-80"
                      style={{
                        borderRight: (i + 1) % 7 !== 0 ? '1px solid var(--border)' : undefined,
                        borderBottom: i < cells.length - 7 ? '1px solid var(--border)' : undefined,
                        backgroundColor: isSelected ? 'var(--accent-alpha)' : isToday ? 'var(--success-alpha)' : 'transparent',
                        opacity: cell.isCurrentMonth ? 1 : 0.35,
                      }}
                    >
                      <div className="flex items-center justify-between mb-1">
                        <span
                          className={'text-xs font-medium' + (isToday ? ' w-5 h-5 rounded-full flex items-center justify-center' : '')}
                          style={{
                            color: isToday ? '#fff' : isSelected ? 'var(--accent)' : 'var(--text-primary)',
                            backgroundColor: isToday ? 'var(--accent)' : undefined,
                          }}
                        >
                          {cell.day}
                        </span>
                        {hasItems && !isSelected && (
                          <span className="text-[9px] font-mono" style={{ color: 'var(--accent)' }}>{dayEvents.length + dayTasks.length}</span>
                        )}
                      </div>
                      {/* Event dots */}
                      <div className="space-y-0.5">
                        {dayEvents.slice(0, 3).map(ev => {
                          const catColor = getCatColor(ev)
                          return (
                            <div key={ev.id} className="text-[9px] leading-tight truncate py-0.5 rounded"
                              style={{
                                backgroundColor: catColor ? catColor + '20' : 'var(--accent-alpha)',
                                color: catColor || 'var(--accent)',
                                borderLeft: `3px solid ${catColor || 'var(--accent)'}`,
                                paddingLeft: '4px',
                                paddingRight: '4px',
                              }}>
                              {ev.time.slice(0, 5)} {ev.title}
                            </div>
                          )
                        })}
                        {dayTasks.slice(0, Math.max(0, 3 - dayEvents.length)).map(t => (
                          <div key={t.id} className="text-[9px] leading-tight truncate px-1 py-0.5 rounded" style={{ backgroundColor: 'var(--warning)' + '20', color: 'var(--warning)' }}>
                            {t.due_time ? t.due_time.slice(0, 5) + ' ' : ''}{t.title}
                          </div>
                        ))}
                        {(dayEvents.length + dayTasks.length) > 3 && (
                          <span className="text-[9px] px-1" style={{ color: 'var(--text-secondary)' }}>+{dayEvents.length + dayTasks.length - 3} mas</span>
                        )}
                      </div>
                    </div>
                  )
                })}
              </div>
            </div>

            {/* Side panel: selected day details */}
            <div className="space-y-3">
              {selectedDay ? (
                <>
                  <div className="rounded-xl p-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
                    <h3 className="text-sm font-semibold mb-1" style={{ color: 'var(--text-primary)' }}>
                      {new Date(selectedDay + 'T12:00:00').toLocaleDateString('es', { weekday: 'long', day: 'numeric', month: 'long' })}
                    </h3>
                    <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>
                      {selectedEvents.length} evento{selectedEvents.length !== 1 ? 's' : ''}, {selectedTasks.length} tarea{selectedTasks.length !== 1 ? 's' : ''}
                    </span>
                  </div>

                  {selectedEvents.length === 0 && selectedTasks.length === 0 && (
                    <div className="rounded-xl p-6 text-center" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
                      <Calendar size={24} className="mx-auto mb-2" style={{ color: 'var(--text-secondary)', opacity: 0.4 }} />
                      <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Sin eventos este dia</p>
                    </div>
                  )}

                  {selectedEvents.map(ev => {
                    const isPast = `${ev.date} ${ev.time}` < new Date().toISOString().slice(0, 16).replace('T', ' ')
                    const catColor = getCatColor(ev)
                    const cat = ev.category ? catMap.get(ev.category) : undefined
                    return (
                      <div key={ev.id} className="rounded-xl p-4" style={{ backgroundColor: 'var(--card-bg)', borderLeft: `4px solid ${catColor || 'var(--accent)'}`, borderTop: '1px solid var(--card-border)', borderRight: '1px solid var(--card-border)', borderBottom: '1px solid var(--card-border)', opacity: isPast ? 0.5 : 1 }}>
                        <div className="flex items-start justify-between gap-2">
                          <div className="min-w-0">
                            <div className="flex items-center gap-2">
                              <span className="text-xs font-bold font-mono" style={{ color: catColor || 'var(--accent)' }}>
                                {ev.time.slice(0, 5)}{ev.end_time ? ` - ${ev.end_time.slice(0, 5)}` : ''}
                              </span>
                              <h4 className="text-sm font-semibold truncate" style={{ color: 'var(--text-primary)' }}>{ev.title}</h4>
                              {ev.notify_telegram === false && <BellOff size={10} style={{ color: 'var(--text-secondary)', opacity: 0.5 }} title="Sin aviso Telegram" />}
                            </div>
                            {ev.description && <p className="text-[10px] mt-1" style={{ color: 'var(--text-primary)', opacity: 0.8 }}>{ev.description}</p>}
                            {ev.location && (
                              <p className="text-[10px] mt-0.5 flex items-center gap-1" style={{ color: 'var(--text-secondary)' }}>
                                <MapPin size={9} /> {ev.location}
                              </p>
                            )}
                            <p className="text-[10px] mt-1" style={{ color: 'var(--text-secondary)' }}>
                              {ev.created_by}
                              {cat && (
                                <span className="ml-1.5 px-1.5 py-0.5 rounded text-[9px] font-medium" style={{ backgroundColor: cat.color + '20', color: cat.color }}>
                                  {cat.name}
                                </span>
                              )}
                              {ev.recurrence && ev.recurrence !== 'none' && (
                                <span className="ml-1.5 px-1 py-0.5 rounded text-[9px]" style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}>
                                  {ev.recurrence === 'daily' ? 'Diario' : ev.recurrence === 'weekly' ? 'Semanal' : 'Mensual'}
                                </span>
                              )}
                            </p>
                            {ev.invitees.length > 0 && (
                              <p className="text-[10px] mt-1" style={{ color: 'var(--text-secondary)' }}>
                                {ev.invitees.map(inv => {
                                  const accepted = ev.accepted.includes(inv)
                                  const declined = ev.declined.includes(inv)
                                  return <span key={inv} className="mr-1.5" style={{ color: accepted ? 'var(--success)' : declined ? 'var(--danger)' : 'var(--text-secondary)' }}>{inv}</span>
                                })}
                              </p>
                            )}
                          </div>
                          <div className="flex items-center gap-1 shrink-0">
                            {user && !ev.accepted.includes(user.username) && !ev.declined.includes(user.username) && ev.created_by !== user.username && (
                              <>
                                <button onClick={async () => { await acceptEvent(ev.id, user!.username); await loadData() }}
                                  className="p-1 rounded-lg" style={{ color: 'var(--success)' }} title="Aceptar"><CheckCircle2 size={14} /></button>
                                <button onClick={async () => { await declineEvent(ev.id, user!.username); await loadData() }}
                                  className="p-1 rounded-lg" style={{ color: 'var(--danger)' }} title="Rechazar"><XCircle size={14} /></button>
                              </>
                            )}
                            <button onClick={() => {
                              setEditingEventId(ev.id); setEventTitle(ev.title); setEventDate(ev.date); setEventTime(ev.time)
                              setEventEndTime(ev.end_time || ''); setEventDescription(ev.description || ''); setEventLocation(ev.location || '')
                              setEventInvitees(ev.invitees); setEventRemind(ev.remind_before_min); setEventNotifyTg(ev.notify_telegram ?? true)
                              setEventRecurrence(ev.recurrence || ''); setEventRecurrenceEnd(ev.recurrence_end || ''); setEventCategory(ev.category || '')
                              setInviteeSearch(''); setShowEventModal(true)
                            }} className="p-1 rounded-lg hover:opacity-80" style={{ color: 'var(--text-secondary)' }}><Pencil size={12} /></button>
                            <button onClick={async () => { await deleteEvent(ev.id); await loadData() }}
                              className="p-1 rounded-lg hover:opacity-80" style={{ color: 'var(--danger)' }}><Trash2 size={12} /></button>
                          </div>
                        </div>
                      </div>
                    )
                  })}

                  {selectedTasks.map(t => (
                    <div key={t.id} className="rounded-xl p-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--warning)' + '40' }}>
                      <div className="flex items-center gap-2">
                        <ClipboardList size={12} style={{ color: 'var(--warning)' }} />
                        <span className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>{t.title}</span>
                        {t.due_time && <span className="text-[10px] font-mono" style={{ color: 'var(--warning)' }}>{t.due_time.slice(0, 5)}</span>}
                        <span className="text-[10px] ml-auto px-1.5 py-0.5 rounded" style={{ backgroundColor: 'var(--warning)' + '20', color: 'var(--warning)' }}>Tarea</span>
                      </div>
                    </div>
                  ))}
                </>
              ) : (
                <div className="rounded-xl p-6 text-center" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
                  <Calendar size={24} className="mx-auto mb-2" style={{ color: 'var(--text-secondary)', opacity: 0.4 }} />
                  <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Selecciona un dia para ver sus eventos</p>
                </div>
              )}
            </div>
          </div>

          {/* Event Modal */}
          {showEventModal && (
            <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
              <div className="rounded-xl p-6 w-full max-w-md mx-4" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}>
                <div className="flex items-center justify-between mb-4">
                  <h3 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>{editingEventId ? 'Editar Evento' : 'Nuevo Evento'}</h3>
                  <button onClick={() => setShowEventModal(false)} style={{ color: 'var(--text-secondary)' }}><X size={20} /></button>
                </div>
                <div className="space-y-3">
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Titulo</label>
                    <input value={eventTitle} onChange={e => setEventTitle(e.target.value)} placeholder="Reunion de equipo"
                      className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                  </div>
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Descripcion</label>
                    <textarea value={eventDescription} onChange={e => setEventDescription(e.target.value)} placeholder="Detalles del evento (opcional)"
                      rows={2} className="w-full px-3 py-2 rounded-lg text-sm outline-none resize-none"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                  </div>
                  <div className="grid grid-cols-3 gap-3">
                    <div>
                      <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Fecha</label>
                      <input type="date" value={eventDate} onChange={e => setEventDate(e.target.value)}
                        className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                    </div>
                    <div>
                      <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Inicio</label>
                      <input type="time" value={eventTime} onChange={e => setEventTime(e.target.value)}
                        className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                    </div>
                    <div>
                      <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Fin</label>
                      <input type="time" value={eventEndTime} onChange={e => setEventEndTime(e.target.value)}
                        className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                    </div>
                  </div>
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Lugar</label>
                    <input value={eventLocation} onChange={e => setEventLocation(e.target.value)} placeholder="Sala de reuniones, Lab 3, etc."
                      className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                  </div>
                  <div>
                    <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Invitados</label>
                    {/* Selected chips */}
                    {eventInvitees.length > 0 && (
                      <div className="flex flex-wrap gap-1.5 mb-2">
                        {eventInvitees.map(name => (
                          <span key={name} className="flex items-center gap-1 text-xs px-2 py-0.5 rounded-full"
                            style={{ backgroundColor: name === 'all' ? 'var(--accent-alpha)' : 'var(--bg-tertiary)', color: name === 'all' ? 'var(--accent)' : 'var(--text-primary)' }}>
                            {name === 'all' ? 'Todos' : name}
                            <button type="button" onClick={() => setEventInvitees(prev => prev.filter(n => n !== name))} className="hover:opacity-60"><X size={10} /></button>
                          </span>
                        ))}
                      </div>
                    )}
                    {/* Search input */}
                    {!eventInvitees.includes('all') && (
                      <div className="relative">
                        <input
                          value={inviteeSearch}
                          onChange={e => setInviteeSearch(e.target.value)}
                          placeholder={eventInvitees.length > 0 ? 'Agregar otro...' : 'Buscar usuario...'}
                          className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                          style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                        />
                        {/* Suggestions */}
                        {inviteeSearch.length > 0 && (() => {
                          const q = inviteeSearch.toLowerCase()
                          const suggestions = allUsers.filter(u => u.toLowerCase().includes(q) && !eventInvitees.includes(u) && u !== user?.username)
                          const showAll = 'todos'.includes(q) || 'all'.includes(q)
                          if (!suggestions.length && !showAll) return null
                          return (
                            <div className="absolute z-10 left-0 right-0 rounded-lg mt-1 overflow-hidden max-h-36 overflow-y-auto shadow-lg"
                              style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--border)' }}>
                              {showAll && (
                                <button type="button" onClick={() => { setEventInvitees(['all']); setInviteeSearch('') }}
                                  className="w-full text-left px-3 py-2 text-xs font-medium hover:opacity-80"
                                  style={{ color: 'var(--accent)', backgroundColor: 'var(--accent-alpha)' }}>
                                  Todos los usuarios
                                </button>
                              )}
                              {suggestions.map(u => (
                                <button type="button" key={u} onClick={() => { setEventInvitees(prev => [...prev, u]); setInviteeSearch('') }}
                                  className="w-full text-left px-3 py-2 text-xs hover:opacity-80 transition-all"
                                  style={{ color: 'var(--text-primary)', borderTop: '1px solid var(--border)' }}>
                                  {u}
                                </button>
                              ))}
                            </div>
                          )
                        })()}
                      </div>
                    )}
                  </div>
                  {/* Categoria */}
                  <div>
                    <div className="flex items-center justify-between mb-1">
                      <label className="block text-xs font-medium" style={{ color: 'var(--text-secondary)' }}>Categoria</label>
                      <button type="button" onClick={() => setShowCategoryManager(!showCategoryManager)}
                        className="text-[10px]" style={{ color: 'var(--accent)' }}>
                        {showCategoryManager ? 'Cerrar' : 'Gestionar'}
                      </button>
                    </div>
                    <div className="flex flex-wrap gap-1.5">
                      <button type="button" onClick={() => setEventCategory('')}
                        className="px-2.5 py-1 rounded-full text-xs font-medium transition-all"
                        style={{
                          backgroundColor: !eventCategory ? 'var(--text-secondary)' : 'var(--bg-tertiary)',
                          color: !eventCategory ? '#fff' : 'var(--text-secondary)',
                        }}>
                        Sin categoria
                      </button>
                      {categories.map(cat => (
                        <button type="button" key={cat.id} onClick={() => setEventCategory(cat.id)}
                          className="px-2.5 py-1 rounded-full text-xs font-medium transition-all"
                          style={{
                            backgroundColor: eventCategory === cat.id ? cat.color : cat.color + '20',
                            color: eventCategory === cat.id ? '#fff' : cat.color,
                            border: eventCategory === cat.id ? `2px solid ${cat.color}` : '2px solid transparent',
                          }}>
                          {cat.name}
                        </button>
                      ))}
                    </div>
                    {/* Category manager inline */}
                    {showCategoryManager && (
                      <div className="mt-2 p-3 rounded-lg space-y-2" style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}>
                        <div className="flex gap-2">
                          <input value={newCatName} onChange={e => setNewCatName(e.target.value)} placeholder="Nueva categoria"
                            className="flex-1 px-2 py-1 rounded-lg text-xs outline-none"
                            style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                          <input type="color" value={newCatColor} onChange={e => setNewCatColor(e.target.value)}
                            className="w-8 h-7 rounded cursor-pointer" style={{ border: '1px solid var(--border)' }} />
                          <button type="button" onClick={async () => {
                            if (!newCatName.trim()) return
                            await createCategory(newCatName, newCatColor)
                            setNewCatName(''); await loadData()
                          }} className="px-2 py-1 rounded-lg text-xs font-medium"
                            style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                            <Plus size={12} />
                          </button>
                        </div>
                        {categories.map(cat => (
                          <div key={cat.id} className="flex items-center gap-2 text-xs py-1">
                            <input type="color" defaultValue={cat.color}
                              onBlur={async (e) => { if (e.target.value !== cat.color) { await updateCategory(cat.id, cat.name, e.target.value); await loadData() } }}
                              className="w-5 h-5 rounded cursor-pointer border-0" />
                            <input type="text" defaultValue={cat.name}
                              onBlur={async (e) => { if (e.target.value.trim() && e.target.value !== cat.name) { await updateCategory(cat.id, e.target.value, cat.color); await loadData() } }}
                              className="flex-1 px-1 py-0.5 rounded text-xs outline-none"
                              style={{ backgroundColor: 'transparent', color: 'var(--text-primary)' }} />
                            <button type="button" onClick={async () => { await deleteCategory(cat.id); await loadData() }}
                              style={{ color: 'var(--danger)' }}><Trash2 size={12} /></button>
                          </div>
                        ))}
                      </div>
                    )}
                  </div>

                  <div className="grid grid-cols-2 gap-3">
                    <div>
                      <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Avisar (min antes)</label>
                      <input type="number" min={1} value={eventRemind} onChange={e => setEventRemind(parseInt(e.target.value) || 15)}
                        className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                    </div>
                    <div>
                      <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Repetir</label>
                      <select value={eventRecurrence} onChange={e => setEventRecurrence(e.target.value)}
                        className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}>
                        <option value="">No repetir</option>
                        <option value="daily">Diario</option>
                        <option value="weekly">Semanal</option>
                        <option value="monthly">Mensual</option>
                      </select>
                    </div>
                  </div>
                  {eventRecurrence && (
                    <div>
                      <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Repetir hasta (opcional)</label>
                      <input type="date" value={eventRecurrenceEnd} onChange={e => setEventRecurrenceEnd(e.target.value)}
                        className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                        style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }} />
                    </div>
                  )}
                  <label className="flex items-center gap-2 cursor-pointer">
                    <input type="checkbox" checked={eventNotifyTg} onChange={e => setEventNotifyTg(e.target.checked)}
                      style={{ accentColor: 'var(--accent)' }} />
                    <span className="text-xs" style={{ color: 'var(--text-secondary)' }}>Avisar por Telegram</span>
                  </label>
                </div>
                <div className="flex justify-end gap-3 mt-5">
                  <button onClick={() => setShowEventModal(false)} className="px-4 py-2 rounded-lg text-sm font-medium"
                    style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>Cancelar</button>
                  <button
                    onClick={async () => {
                      if (!eventTitle.trim() || !eventDate || !eventTime) return
                      const data = {
                        title: eventTitle, date: eventDate, time: eventTime, end_time: eventEndTime || undefined,
                        description: eventDescription, location: eventLocation || undefined, invitees: eventInvitees,
                        remind_before_min: eventRemind, notify_telegram: eventNotifyTg,
                        recurrence: eventRecurrence || undefined, recurrence_end: eventRecurrenceEnd || null,
                        category: eventCategory || undefined,
                      }
                      if (editingEventId) {
                        await updateEvent(editingEventId, data)
                      } else {
                        await createEvent(data)
                      }
                      setShowEventModal(false)
                      await loadData()
                    }}
                    disabled={!eventTitle.trim() || !eventDate || !eventTime}
                    className="px-4 py-2 rounded-lg text-sm font-medium"
                    style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
                  >{editingEventId ? 'Guardar' : 'Crear'}</button>
                </div>
              </div>
            </div>
          )}
        </div>
        )
      })()}

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

        {/* Panel de miembros y tags */}
        {selectedProject && (() => {
          const project = projects.find((p) => p.id === selectedProject)
          if (!project) return null
          return (
            <div style={{ borderTop: '1px solid var(--border)' }}>
              <button
                onClick={() => setShowMembers(!showMembers)}
                className="w-full px-4 py-2.5 flex items-center gap-2 text-xs font-medium"
                style={{ color: 'var(--text-secondary)' }}
              >
                <Users size={14} />
                Miembros ({project.members.length})
                <span className="ml-auto text-[10px]">{showMembers ? '▲' : '▼'}</span>
              </button>
              {showMembers && (
                <div className="px-3 pb-3 space-y-2">
                  {project.members.map((member) => (
                    <div key={member} className="p-2 rounded-lg" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
                      <div className="flex items-center justify-between">
                        <span className="text-xs font-medium" style={{ color: 'var(--text-primary)' }}>@{member}</span>
                        {member !== project.created_by && (
                          <button
                            onClick={() => handleRemoveMember(project.id, member)}
                            className="p-0.5 rounded hover:opacity-80"
                            style={{ color: 'var(--danger)' }}
                          ><X size={12} /></button>
                        )}
                      </div>
                      {/* Tags del miembro */}
                      <div className="flex flex-wrap gap-1 mt-1">
                        {(project.member_tags?.[member] || []).map((tag) => (
                          <span
                            key={tag}
                            className="inline-flex items-center gap-0.5 px-1.5 py-0.5 rounded text-[10px] font-medium cursor-pointer hover:opacity-70"
                            style={{ backgroundColor: 'var(--accent-alpha)', color: 'var(--accent)' }}
                            onClick={() => handleRemoveTag(project.id, member, tag)}
                            title="Click para quitar"
                          >
                            <Tag size={8} />
                            {tag}
                            <X size={8} />
                          </span>
                        ))}
                        {newTagUser === member ? (
                          <input
                            type="text"
                            value={newTagValue}
                            onChange={(e) => setNewTagValue(e.target.value)}
                            onKeyDown={(e) => { if (e.key === 'Enter') handleAddTag(project.id, member); if (e.key === 'Escape') { setNewTagUser(''); setNewTagValue('') } }}
                            onBlur={() => { if (!newTagValue) setNewTagUser('') }}
                            placeholder="tag..."
                            className="px-1.5 py-0.5 rounded text-[10px] outline-none w-16"
                            style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                            autoFocus
                          />
                        ) : (
                          <button
                            onClick={() => { setNewTagUser(member); setNewTagValue('') }}
                            className="inline-flex items-center px-1 py-0.5 rounded text-[10px] hover:opacity-80"
                            style={{ color: 'var(--text-secondary)', border: '1px dashed var(--border)' }}
                          >
                            <Plus size={8} />
                          </button>
                        )}
                      </div>
                    </div>
                  ))}
                  {/* Agregar miembro */}
                  <div className="flex gap-1.5">
                    <input
                      type="text"
                      value={newMember}
                      onChange={(e) => setNewMember(e.target.value)}
                      onKeyDown={(e) => e.key === 'Enter' && handleAddMember(project.id)}
                      placeholder="Agregar miembro..."
                      className="flex-1 px-2 py-1 rounded-lg text-xs outline-none"
                      style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                    />
                    <button
                      onClick={() => handleAddMember(project.id)}
                      disabled={!newMember.trim()}
                      className="p-1 rounded-lg"
                      style={{ color: 'var(--accent)' }}
                    >
                      <UserPlus size={14} />
                    </button>
                  </div>
                </div>
              )}
            </div>
          )
        })()}
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

              {/* Fecha y hora limite */}
              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Fecha (opcional)</label>
                  <input
                    type="date"
                    value={taskDueDate}
                    onChange={(e) => setTaskDueDate(e.target.value)}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                  />
                </div>
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Hora (opcional)</label>
                  <input
                    type="time"
                    value={taskDueTime}
                    onChange={(e) => setTaskDueTime(e.target.value)}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                  />
                </div>
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

      {/* Modal de agendar tarea */}
      {showScheduleModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="rounded-xl p-6 w-full max-w-sm mx-4" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}>
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>Agendar Tarea</h3>
              <button onClick={() => setShowScheduleModal(false)} style={{ color: 'var(--text-secondary)' }}><X size={18} /></button>
            </div>
            <p className="text-xs mb-4" style={{ color: 'var(--text-secondary)' }}>
              Completa {scheduleMissing === 'both' ? 'la fecha y hora' : scheduleMissing === 'date' ? 'la fecha' : 'la hora'} para crear la actividad en el calendario.
            </p>
            <div className="space-y-3">
              {(scheduleMissing === 'both' || scheduleMissing === 'date') && (
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Fecha</label>
                  <input
                    type="date"
                    value={scheduleDate}
                    onChange={(e) => setScheduleDate(e.target.value)}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                    autoFocus
                  />
                </div>
              )}
              {(scheduleMissing === 'both' || scheduleMissing === 'time') && (
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Hora</label>
                  <input
                    type="time"
                    value={scheduleTime}
                    onChange={(e) => setScheduleTime(e.target.value)}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                    style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                  />
                </div>
              )}
            </div>
            <div className="flex justify-end gap-3 mt-5">
              <button
                onClick={() => setShowScheduleModal(false)}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
              >Cancelar</button>
              <button
                onClick={handleScheduleConfirm}
                disabled={!scheduleDate || !scheduleTime}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ backgroundColor: 'var(--accent)', color: '#ffffff', opacity: scheduleDate && scheduleTime ? 1 : 0.5 }}
              >Agendar</button>
            </div>
          </div>
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
                {task.due_date}{task.due_time ? ` ${task.due_time}` : ''}
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
