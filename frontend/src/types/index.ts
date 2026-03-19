export interface FileEntry {
  name: string
  path: string
  is_dir: boolean
  size: number
  modified: string
  extension: string | null
}

export interface NetworkHost {
  ip: string
  hostname: string | null
  is_alive: boolean
  last_seen: string
  response_time_ms: number | null
}

export interface StorageInfo {
  total_files: number
  total_dirs: number
  total_size: number
  path: string
}

export interface QuickAccess {
  name: string
  path: string
  icon: string
}

export interface DiskInfo {
  name: string
  mount_point: string
  total_space: number
  available_space: number
  used_space: number
  file_system: string
  is_removable: boolean
}

export interface SystemInfo {
  hostname: string
  local_ip: string
  os: string
  kernel: string
  total_memory: number
  used_memory: number
  cpu_count: number
  uptime_secs: number
}

// --- System ---

export interface AutostartStatus {
  installed: boolean
  enabled: boolean
}

// --- Printers 3D ---

export interface Printer3DConfig {
  id: string
  name: string
  ip: string
  port: number
  printer_type: 'OctoPrint' | 'Moonraker'
  api_key: string | null
}

export interface Printer3DStatus {
  id: string
  online: boolean
  temperatures: PrinterTemps | null
  current_job: PrintJob | null
}

export interface PrinterTemps {
  hotend_actual: number
  hotend_target: number
  bed_actual: number
  bed_target: number
}

export interface PrintJob {
  file_name: string
  progress: number
  time_elapsed: number | null
  time_remaining: number | null
  state: string
}

export interface AddPrinter3DRequest {
  name: string
  ip: string
  port: number
  printer_type: 'OctoPrint' | 'Moonraker'
  api_key: string | null
}

export interface DetectPrintersResult {
  ip: string
  port: number
  printer_type: 'OctoPrint' | 'Moonraker'
  name: string | null
}

// --- CUPS Printing ---

export interface CupsPrinter {
  name: string
  description: string
  is_default: boolean
  state: string
}

export interface CupsPrintJob {
  id: string
  printer: string
  title: string
  state: string
  size: string | null
}

export interface PrintFileRequest {
  path: string
  printer: string
  copies?: number
  orientation?: string
  double_sided?: boolean
  pages?: string
}
