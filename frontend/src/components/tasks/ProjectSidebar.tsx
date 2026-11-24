import { useState } from 'react'
import { Plus, Trash2, FolderOpen, ClipboardList } from 'lucide-react'
import { createProject, deleteProject } from '../../api'
import type { Task, Project } from '../../types'
import type { MembersPanelState } from './useMembersPanel'
import ProjectMembersPanel from './ProjectMembersPanel'

interface Props {
  projects: Project[]
  tasks: Task[]
  selectedProject: string | null
  setSelectedProject: (id: string | null) => void
  membersState: MembersPanelState
  loadData: () => Promise<void>
}

export default function ProjectSidebar({ projects, tasks, selectedProject, setSelectedProject, membersState, loadData }: Props) {
  const [showNewProject, setShowNewProject] = useState(false)
  const [newProjectName, setNewProjectName] = useState('')

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

  const selected = selectedProject ? projects.find((p) => p.id === selectedProject) : undefined

  return (
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
      {selected && <ProjectMembersPanel project={selected} state={membersState} loadData={loadData} />}
    </div>
  )
}
