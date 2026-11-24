import { useState } from 'react'
import { Plus, X } from 'lucide-react'
import { createTask } from '../../api'
import type { Project } from '../../types'

interface Props {
  projects: Project[]
  allUsers: string[]
  defaultProjectId: string
  onClose: () => void
  loadData: () => Promise<void>
}

export default function TaskModal({ projects, allUsers, defaultProjectId, onClose, loadData }: Props) {
  const [taskTitle, setTaskTitle] = useState('')
  const [taskProjectId, setTaskProjectId] = useState<string>(defaultProjectId)
  const [taskAssignedTo, setTaskAssignedTo] = useState('')
  const [taskDueDate, setTaskDueDate] = useState('')
  const [taskRequiresConfirmation, setTaskRequiresConfirmation] = useState(false)
  const [taskInsistent, setTaskInsistent] = useState(false)
  const [taskReminderMinutes, setTaskReminderMinutes] = useState(8)
  const [taskDueTime, setTaskDueTime] = useState('')

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
      onClose()
      loadData()
    } catch { /* silenciar */ }
  }

  return (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div
            className="rounded-xl p-6 w-full max-w-md mx-4"
            style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
          >
            <div className="flex items-center justify-between mb-5">
              <h3 className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>Nueva Tarea</h3>
              <button onClick={() => onClose()} style={{ color: 'var(--text-secondary)' }}>
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
                onClick={() => onClose()}
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
  )
}
