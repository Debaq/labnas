import { Plus, Clock, CheckCircle2, ClipboardList } from 'lucide-react'
import type { Task, Project } from '../../types'
import TaskCard from './TaskCard'
import { STATUS_CONFIG, STATUS_ORDER } from './utils'

interface Props {
  filteredTasks: Task[]
  projects: Project[]
  onNewTask: () => void
  onConfirm: (id: string) => void
  onReject: (id: string) => void
  onDone: (id: string) => void
  onDelete: (id: string) => void
}

export default function TaskListPanel({ filteredTasks, projects, onNewTask, onConfirm, onReject, onDone, onDelete }: Props) {
  // Estadisticas
  const totalTasks = filteredTasks.length
  const pendingTasks = filteredTasks.filter((t) => t.status === 'pendiente' || t.status === 'enprogreso').length
  const completedTasks = filteredTasks.filter((t) => t.status === 'completada').length
  const progress = totalTasks > 0 ? Math.round((completedTasks / totalTasks) * 100) : 0

  return (
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
            onClick={onNewTask}
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
                        onConfirm={() => onConfirm(task.id)}
                        onReject={() => onReject(task.id)}
                        onDone={() => onDone(task.id)}
                        onDelete={() => onDelete(task.id)}
                      />
                    ))}
                  </div>
                </div>
              )
            })
          )}
        </div>
      </div>
  )
}
