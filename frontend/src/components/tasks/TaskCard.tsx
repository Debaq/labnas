import { Trash2, CheckCircle2, XCircle, FolderOpen, Bell, ShieldCheck, Calendar, Users } from 'lucide-react'
import type { Task, Project } from '../../types'

// Componente de tarjeta de tarea
export default function TaskCard({
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
