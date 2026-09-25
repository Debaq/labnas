import { api } from './client'
import type { Task, Project, CalendarEvent } from '../types'

// --- Tasks & Projects ---

export async function fetchProjects(): Promise<Project[]> {
  const res = await api('/api/projects')
  if (!res.ok) throw new Error('Error al obtener proyectos')
  return res.json()
}

export async function createProject(data: { name: string; description?: string }): Promise<Project> {
  const res = await api('/api/projects', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al crear proyecto')
  return res.json()
}

export async function updateProject(id: string, data: {
  name?: string; description?: string; members?: string[];
  member_tags?: Record<string, string[]>
}): Promise<Project> {
  const res = await api(`/api/projects/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al actualizar proyecto')
  return res.json()
}

export async function deleteProject(id: string): Promise<void> {
  const res = await api(`/api/projects/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar proyecto')
}

export async function fetchTasks(params?: { project?: string; status?: string }): Promise<Task[]> {
  const query = new URLSearchParams()
  if (params?.project) query.set('project', params.project)
  if (params?.status) query.set('status', params.status)
  const qs = query.toString()
  const res = await api(`/api/tasks${qs ? '?' + qs : ''}`)
  if (!res.ok) throw new Error('Error al obtener tareas')
  return res.json()
}

export async function createTask(data: {
  title: string
  project_id?: string | null
  assigned_to?: string[]
  requires_confirmation?: boolean
  insistent?: boolean
  reminder_minutes?: number
  due_date?: string | null
  due_time?: string | null
}): Promise<Task> {
  const res = await api('/api/tasks', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al crear tarea')
  return res.json()
}

export async function updateTask(id: string, data: Record<string, unknown>): Promise<Task> {
  const res = await api(`/api/tasks/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al actualizar tarea')
  return res.json()
}

export async function confirmTask(id: string, user: string): Promise<Task> {
  const res = await api(`/api/tasks/${id}/confirm`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ user }),
  })
  if (!res.ok) throw new Error('Error al confirmar tarea')
  return res.json()
}

export async function rejectTask(id: string, user: string): Promise<Task> {
  const res = await api(`/api/tasks/${id}/reject`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ user }),
  })
  if (!res.ok) throw new Error('Error al rechazar tarea')
  return res.json()
}

export async function doneTask(id: string): Promise<Task> {
  const res = await api(`/api/tasks/${id}/done`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al completar tarea')
  return res.json()
}

export async function deleteTask(id: string): Promise<void> {
  const res = await api(`/api/tasks/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar tarea')
}

export async function scheduleTask(id: string, data: { date?: string; time?: string }): Promise<CalendarEvent> {
  const res = await api(`/api/tasks/${id}/schedule`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al agendar tarea')
  return res.json()
}

// --- Calendar Events ---

export async function fetchEvents(): Promise<CalendarEvent[]> {
  const res = await api('/api/events')
  if (!res.ok) throw new Error('Error al obtener eventos')
  return res.json()
}

export async function updateEvent(id: string, data: Record<string, unknown>): Promise<CalendarEvent> {
  const res = await api(`/api/events/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al actualizar evento')
  return res.json()
}

export async function createEvent(data: {
  title: string; date: string; time: string; end_time?: string;
  description?: string; location?: string; invitees?: string[]; remind_before_min?: number;
  notify_telegram?: boolean; recurrence?: string; recurrence_end?: string | null; category?: string
}): Promise<CalendarEvent> {
  const res = await api('/api/events', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  })
  if (!res.ok) throw new Error('Error al crear evento')
  return res.json()
}

export async function deleteEvent(id: string): Promise<void> {
  const res = await api(`/api/events/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar evento')
}

export async function acceptEvent(id: string, user: string): Promise<CalendarEvent> {
  const res = await api(`/api/events/${id}/accept`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ user }),
  })
  if (!res.ok) throw new Error('Error al aceptar evento')
  return res.json()
}

export async function declineEvent(id: string, user: string): Promise<CalendarEvent> {
  const res = await api(`/api/events/${id}/decline`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ user }),
  })
  if (!res.ok) throw new Error('Error al rechazar evento')
  return res.json()
}

// --- Event Categories ---

export async function fetchCategories(): Promise<import('../types').EventCategory[]> {
  const res = await api('/api/events/categories')
  if (!res.ok) throw new Error('Error al obtener categorias')
  return res.json()
}

export async function createCategory(name: string, color: string): Promise<import('../types').EventCategory> {
  const res = await api('/api/events/categories', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, color }),
  })
  if (!res.ok) throw new Error('Error al crear categoria')
  return res.json()
}

export async function updateCategory(id: string, name: string, color: string): Promise<import('../types').EventCategory> {
  const res = await api(`/api/events/categories/${id}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, color }),
  })
  if (!res.ok) throw new Error('Error al actualizar categoria')
  return res.json()
}

export async function deleteCategory(id: string): Promise<void> {
  const res = await api(`/api/events/categories/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al eliminar categoria')
}
