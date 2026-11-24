import { useEffect, useState, useCallback } from 'react'
import { Loader2, ClipboardList, Calendar } from 'lucide-react'
import {
  fetchProjects,
  fetchTasks,
  confirmTask,
  rejectTask,
  doneTask,
  deleteTask,
  fetchEvents,
  fetchUsernames,
  fetchCategories,
} from '../api'
import type { Task, Project, CalendarEvent, EventCategory } from '../types'
import { useAuth } from '../auth/AuthContext'
import ProjectSidebar from '../components/tasks/ProjectSidebar'
import TaskListPanel from '../components/tasks/TaskListPanel'
import TaskModal from '../components/tasks/TaskModal'
import ScheduleModal from '../components/tasks/ScheduleModal'
import CalendarView from '../components/tasks/CalendarView'
import { useEventForm } from '../components/tasks/useEventForm'
import { useMembersPanel } from '../components/tasks/useMembersPanel'
import { useScheduleModal } from '../components/tasks/useScheduleModal'

export default function TasksPage() {
  const { user } = useAuth()
  const [tab, setTab] = useState<'tareas' | 'calendario'>('tareas')
  const [projects, setProjects] = useState<Project[]>([])
  const [tasks, setTasks] = useState<Task[]>([])
  const [loading, setLoading] = useState(true)
  const [selectedProject, setSelectedProject] = useState<string | null>(null)
  const [showTaskModal, setShowTaskModal] = useState(false)
  const [events, setEvents] = useState<CalendarEvent[]>([])
  const [categories, setCategories] = useState<EventCategory[]>([])
  const [allUsers, setAllUsers] = useState<string[]>([])

  // Calendario
  const [calendarMonth, setCalendarMonth] = useState(() => { const d = new Date(); return new Date(d.getFullYear(), d.getMonth(), 1) })
  const [selectedDay, setSelectedDay] = useState<string | null>(null)
  const eventForm = useEventForm()

  // Panel de miembros/tags del proyecto
  const membersState = useMembersPanel()

  // Modal de agendar tarea (cuando falta fecha/hora al confirmar)
  const schedule = useScheduleModal()

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
      schedule.open(id, task)
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
        <CalendarView
          events={events}
          tasks={tasks}
          categories={categories}
          allUsers={allUsers}
          username={user?.username}
          calendarMonth={calendarMonth}
          setCalendarMonth={setCalendarMonth}
          selectedDay={selectedDay}
          setSelectedDay={setSelectedDay}
          eventForm={eventForm}
          loadData={loadData}
        />
      )}

      {/* Tareas */}
      {tab === 'tareas' && (
        <div className="flex gap-6 h-full" style={{ minHeight: 0 }}>
          <ProjectSidebar
            projects={projects}
            tasks={tasks}
            selectedProject={selectedProject}
            setSelectedProject={setSelectedProject}
            membersState={membersState}
            loadData={loadData}
          />

          <TaskListPanel
            filteredTasks={filteredTasks}
            projects={projects}
            onNewTask={() => setShowTaskModal(true)}
            onConfirm={handleConfirm}
            onReject={handleReject}
            onDone={handleDone}
            onDelete={handleDelete}
          />

          {showTaskModal && (
            <TaskModal
              projects={projects}
              allUsers={allUsers}
              defaultProjectId={selectedProject || ''}
              onClose={() => setShowTaskModal(false)}
              loadData={loadData}
            />
          )}
        </div>
      )}

      <ScheduleModal schedule={schedule} loadData={loadData} />
    </div>
  )
}
