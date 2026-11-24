import { X, Users, Tag, UserPlus, Plus } from 'lucide-react'
import { updateProject } from '../../api'
import type { Project } from '../../types'
import type { MembersPanelState } from './useMembersPanel'

interface Props {
  project: Project
  state: MembersPanelState
  loadData: () => Promise<void>
}

export default function ProjectMembersPanel({ project, state, loadData }: Props) {
  const { showMembers, setShowMembers, newMember, setNewMember, newTagUser, setNewTagUser, newTagValue, setNewTagValue } = state

  async function handleAddMember(projectId: string) {
    if (!newMember.trim()) return
    const members = [...project.members, newMember.trim()]
    try {
      await updateProject(projectId, { members })
      setNewMember('')
      loadData()
    } catch { /* silenciar */ }
  }

  async function handleRemoveMember(projectId: string, member: string) {
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
    const memberTags = { ...project.member_tags }
    memberTags[username] = (memberTags[username] || []).filter((t) => t !== tag)
    if (memberTags[username].length === 0) delete memberTags[username]
    try {
      await updateProject(projectId, { member_tags: memberTags })
      loadData()
    } catch { /* silenciar */ }
  }

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
}
