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
  mac: string | null
  vendor: string | null
  is_alive: boolean
  is_known: boolean
  label: string | null
  icon: string | null
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
  install_cmd: string
  uninstall_cmd: string
}

// --- Notifications (Telegram) ---

export type UserRole = 'pendiente' | 'observador' | 'operador' | 'admin'

export interface UserPermissions {
  terminal: boolean
  impresion: boolean
  archivos_escritura: boolean
}

export interface TelegramChat {
  chat_id: number
  name: string
  username: string | null
  role: UserRole
  permissions: UserPermissions
  linked_web_user: string | null
  daily_enabled: boolean
  daily_hour: number
  daily_minute: number
}

export interface NotificationConfig {
  bot_configured: boolean
  bot_username: string | null
  telegram_chats: TelegramChat[]
  daily_enabled: boolean
  daily_hour: number
  daily_minute: number
}

// --- Printers 3D ---

export interface Printer3DConfig {
  id: string
  name: string
  ip: string
  port: number
  printer_type: 'OctoPrint' | 'Moonraker' | 'CrealityStock' | 'FlashForge'
  api_key: string | null
  camera_url: string | null
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
  printer_type: 'OctoPrint' | 'Moonraker' | 'CrealityStock' | 'FlashForge'
  api_key: string | null
  camera_url: string | null
}

export interface PrinterFileInfo {
  name: string
  size: number | null
  date: number | null
}

export interface DetectPrintersResult {
  ip: string
  port: number
  printer_type: 'OctoPrint' | 'Moonraker' | 'CrealityStock' | 'FlashForge'
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

export interface PrinterOption {
  key: string
  display_name: string
  default_value: string
  values: string[]
}

export interface PrintFileRequest {
  path: string
  printer: string
  copies?: number
  pages?: string
  options: Record<string, string>
}

// --- Tasks & Projects ---

export type TaskStatus = 'pendiente' | 'enprogreso' | 'completada' | 'rechazada'

export interface Task {
  id: string
  project_id: string | null
  title: string
  description: string
  assigned_to: string[]
  status: TaskStatus
  created_by: string
  due_date: string | null
  due_time: string | null
  requires_confirmation: boolean
  insistent: boolean
  reminder_minutes: number
  confirmed_by: string[]
  rejected_by: string[]
  created_at: string
}

export interface CalendarEvent {
  id: string
  title: string
  description: string
  date: string
  time: string
  created_by: string
  invitees: string[]
  accepted: string[]
  declined: string[]
  remind_before_min: number
  recurrence: string
  recurrence_end: string | null
  created_at: string
}

export interface Project {
  id: string
  name: string
  description: string
  created_by: string
  members: string[]
  member_tags: Record<string, string[]>
  created_at: string
}
