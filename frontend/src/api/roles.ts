import { api, jsonOrThrow } from './client'
import type { ModuleLevel, RoleInfo, RolesResponse } from '../types'

export async function fetchRoles(): Promise<RolesResponse> {
  return jsonOrThrow(await api('/api/roles'), 'Error al cargar los roles')
}

export async function createRole(name: string, copyFrom?: string, description = ''): Promise<RoleInfo> {
  return jsonOrThrow(await api('/api/roles', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ name, description, copy_from: copyFrom || null }),
  }), 'Error al crear el rol')
}

export async function updateRole(id: string, data: { name: string; description: string; expiry_days: number | null; levels: Record<string, ModuleLevel> }): Promise<RoleInfo> {
  return jsonOrThrow(await api(`/api/roles/${encodeURIComponent(id)}`, {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(data),
  }), 'Error al guardar el rol')
}

export async function resetRole(id: string): Promise<RoleInfo> {
  return jsonOrThrow(await api(`/api/roles/${encodeURIComponent(id)}/reset`, { method: 'POST' }), 'Error al restaurar el rol')
}

export async function deleteRole(id: string): Promise<void> {
  const res = await api(`/api/roles/${encodeURIComponent(id)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text().catch(() => '')) || 'Error al borrar el rol')
}
