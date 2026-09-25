import { api } from './client'
import type { CupsPrinter, CupsPrintJob, PrintFileRequest } from '../types'

// --- CUPS Printing ---

export async function fetchCupsPrinters(): Promise<CupsPrinter[]> {
  const res = await api('/api/printing/printers')
  if (!res.ok) throw new Error('Error al obtener impresoras CUPS')
  return res.json()
}

export async function enablePrinter(name: string): Promise<void> {
  const res = await api(`/api/printing/printers/${encodeURIComponent(name)}/enable`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al habilitar impresora')
}

export async function disablePrinter(name: string): Promise<void> {
  const res = await api(`/api/printing/printers/${encodeURIComponent(name)}/disable`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al deshabilitar impresora')
}

export async function fetchPrinterOptions(name: string): Promise<import('../types').PrinterOption[]> {
  const res = await api(`/api/printing/printers/${encodeURIComponent(name)}/options`)
  if (!res.ok) throw new Error('Error al obtener opciones de impresora')
  return res.json()
}

export async function printFileUpload(file: File, printer: string, opts?: {
  copies?: number
  pages?: string
  options?: Record<string, string>
}): Promise<void> {
  const formData = new FormData()
  formData.append('file', file)
  formData.append('printer', printer)
  if (opts?.copies) formData.append('copies', opts.copies.toString())
  if (opts?.pages) formData.append('pages', opts.pages)
  if (opts?.options) {
    for (const [key, value] of Object.entries(opts.options)) {
      formData.append(`opt_${key}`, value)
    }
  }
  const res = await api('/api/printing/print', {
    method: 'POST',
    body: formData,
  })
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    throw new Error(text || 'Error al imprimir')
  }
}

export async function printFilePath(req: PrintFileRequest): Promise<void> {
  const res = await api('/api/printing/print-file', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  })
  if (!res.ok) throw new Error('Error al imprimir archivo')
}

export async function fetchPrintJobs(): Promise<CupsPrintJob[]> {
  const res = await api('/api/printing/jobs')
  if (!res.ok) throw new Error('Error al obtener cola de impresion')
  return res.json()
}

export async function cancelPrintJob(id: string): Promise<void> {
  const res = await api(`/api/printing/jobs/${id}`, { method: 'DELETE' })
  if (!res.ok) throw new Error('Error al cancelar trabajo')
}

export async function wakePrinter(name: string): Promise<void> {
  const res = await api(`/api/printing/printers/${encodeURIComponent(name)}/wake`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al despertar impresora')
}

export async function fetchPrinterStats(name: string): Promise<import('../types').PrinterStatsResponse> {
  const res = await api(`/api/printing/printers/${encodeURIComponent(name)}/stats`)
  if (!res.ok) throw new Error('Error al obtener estadisticas')
  return res.json()
}

export async function setPrinterCosts(name: string, costs: import('../types').PrinterCosts): Promise<void> {
  const res = await api(`/api/printing/printers/${encodeURIComponent(name)}/costs`, {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(costs),
  })
  if (!res.ok) throw new Error('Error al guardar costos')
}

export async function resetPrinterStats(name: string): Promise<void> {
  const res = await api(`/api/printing/printers/${encodeURIComponent(name)}/stats/reset`, { method: 'POST' })
  if (!res.ok) throw new Error('Error al resetear estadisticas')
}

export async function fetchAllUserCosts(): Promise<import('../types').AllUserCostsResponse> {
  const res = await api('/api/printing/user-costs')
  if (!res.ok) throw new Error('Error al obtener costos por usuario')
  return res.json()
}

export async function fetchMyCosts(): Promise<import('../types').AllUserCostsResponse> {
  const res = await api('/api/printing/my-costs')
  if (!res.ok) throw new Error('Error al obtener mis costos')
  return res.json()
}

// --- Duplex Manual ---

export async function duplexPrepare(file: File): Promise<import('../types').DuplexPrepareResponse> {
  const formData = new FormData()
  formData.append('file', file)
  const res = await api('/api/printing/duplex/prepare', {
    method: 'POST',
    body: formData,
  })
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    throw new Error(text || 'Error al preparar archivo para duplex')
  }
  return res.json()
}

export async function duplexPrintStep(req: import('../types').DuplexPrintStepRequest): Promise<void> {
  const res = await api('/api/printing/duplex/print', {
    method: 'POST',
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(req),
  })
  if (!res.ok) {
    const text = await res.text().catch(() => res.statusText)
    throw new Error(text || 'Error al imprimir paso duplex')
  }
}

export async function duplexCleanup(tempId: string): Promise<void> {
  await api(`/api/printing/duplex/${encodeURIComponent(tempId)}`, { method: 'DELETE' })
}
