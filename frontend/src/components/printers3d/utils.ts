import type { Printer3DConfig } from '../../types'

export function formatTime(seconds: number | null | undefined): string {
  if (!seconds) return '--'
  const h = Math.floor(seconds / 3600)
  const m = Math.floor((seconds % 3600) / 60)
  return h > 0 ? `${h}h ${m}m` : `${m}m`
}

export function formatFileSize(bytes: number | null | undefined): string {
  if (!bytes) return '--'
  if (bytes < 1024) return `${bytes} B`
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`
  return `${(bytes / (1024 * 1024)).toFixed(1)} MB`
}

export function printerWebUrl(printer: Printer3DConfig): string | null {
  switch (printer.printer_type) {
    case 'OctoPrint': return `http://${printer.ip}:${printer.port}`
    case 'Moonraker': return `http://${printer.ip}`
    case 'CrealityStock': return `http://${printer.ip}`
    case 'FlashForge': return null
  }
}
