import { api } from './client'
import type { CamerasResponse, CameraBody, CameraDetect, CameraRecording, RecordingNow } from '../types'

async function failText(res: Response, fallback: string): Promise<never> {
  const text = await res.text().catch(() => '')
  throw new Error(text || fallback)
}

export async function fetchCameras(): Promise<CamerasResponse> {
  const res = await api('/api/cameras')
  if (!res.ok) return failText(res, 'Error al obtener camaras')
  return res.json()
}

export async function createCamera(body: CameraBody): Promise<void> {
  const res = await api('/api/cameras', { method: 'POST', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
  if (!res.ok) return failText(res, 'Error al agregar la camara')
}

export async function updateCamera(id: string, body: CameraBody): Promise<void> {
  const res = await api(`/api/cameras/${id}`, { method: 'PUT', headers: { 'Content-Type': 'application/json' }, body: JSON.stringify(body) })
  if (!res.ok) return failText(res, 'Error al guardar la camara')
}

export async function deleteCamera(id: string): Promise<void> {
  const res = await api(`/api/cameras/${id}`, { method: 'DELETE' })
  if (!res.ok) return failText(res, 'Error al eliminar la camara')
}

export async function saveCameraSettings(folder: string, maxMinutes: number): Promise<void> {
  const res = await api('/api/cameras/settings', {
    method: 'PUT',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify({ folder, max_minutes: maxMinutes }),
  })
  if (!res.ok) return failText(res, 'Error al guardar los ajustes')
}

export async function detectCameraDevices(): Promise<CameraDetect> {
  const res = await api('/api/cameras/detect')
  if (!res.ok) return failText(res, 'Error al detectar dispositivos')
  return res.json()
}

export async function startCameraRecording(id: string): Promise<void> {
  const res = await api(`/api/cameras/${id}/record`, { method: 'POST' })
  if (!res.ok) return failText(res, 'Error al empezar a grabar')
}

export async function stopCameraRecording(id: string): Promise<void> {
  const res = await api(`/api/cameras/${id}/stop`, { method: 'POST' })
  if (!res.ok) return failText(res, 'Error al detener la grabacion')
}

export async function takeCameraPhoto(id: string): Promise<string> {
  const res = await api(`/api/cameras/${id}/photo`, { method: 'POST' })
  if (!res.ok) return failText(res, 'Error al tomar la foto')
  return (await res.json()).path
}

export async function fetchRecordingNow(): Promise<RecordingNow[]> {
  const res = await api('/api/cameras/recording-now')
  if (!res.ok) return []
  return res.json()
}

export async function fetchCameraRecordings(): Promise<CameraRecording[]> {
  const res = await api('/api/cameras/recordings')
  if (!res.ok) return failText(res, 'Error al obtener grabaciones')
  return res.json()
}

export async function deleteCameraRecording(id: string): Promise<void> {
  const res = await api(`/api/cameras/recordings/${id}`, { method: 'DELETE' })
  if (!res.ok) return failText(res, 'Error al borrar')
}

export const cameraStreamUrl = (id: string) => `/api/cameras/${id}/stream`
export const cameraFrameUrl = (id: string, t: number) => `/api/cameras/${id}/snapshot?t=${t}`
export const cameraRecordingUrl = (id: string) => `/api/cameras/recordings/${id}/download`
