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

export interface PrinterCosts {
  ink_per_page: number
  paper_carta: number
  paper_oficio: number
  paper_special: number
}

export interface PrinterStats {
  total_jobs: number
  total_pages: number
  pages_carta: number
  pages_oficio: number
  pages_special: number
}

export interface PrinterStatsResponse {
  costs: PrinterCosts
  stats: PrinterStats
  estimated_cost: number
}

export interface UserPrinterStats {
  printer: string
  stats: PrinterStats
  estimated_cost: number
}

export interface UserCostsResponse {
  username: string
  total_cost: number
  total_jobs: number
  total_pages: number
  printers: UserPrinterStats[]
}

export interface AllUserCostsResponse {
  users: UserCostsResponse[]
  general_cost: number
  general_jobs: number
  general_pages: number
}

// --- Inventory ---

export interface InventoryCategory {
  id: string
  name: string
  icon: string
  description: string
}

export type ItemStatus = 'activo' | 'mantenimiento' | 'retirado' | 'agotado'

export interface InventoryItem {
  id: string
  category_id: string
  name: string
  description: string
  quantity: number
  unit: string
  cost: number
  supplier: string
  supplier_url: string
  location: string
  serial_number: string
  purchase_date: string
  status: ItemStatus
  notes: string
  brand: string
  model_name: string
  material_type: string | null
  filament_diameter: number | null
  filament_weight: number | null
  filament_remaining: number | null
  filament_color: string | null
  filament_density: number | null
  custom_fields: Record<string, string>
  created_at: string
}

export interface PrintHistoryEntry {
  id: string
  printer_id: string
  file_name: string
  filament_id: string | null
  weight_grams: number
  print_time_minutes: number
  material_cost: number
  electricity_cost: number
  total_cost: number
  success: boolean
  notes: string
  created_at: string
}

// --- Portfolio ---

export type PortfolioType = 'project' | 'course' | 'diploma' | 'workshop'
export type PortfolioStatus = 'planned' | 'active' | 'completed' | 'cancelled' | 'submitted'
export type PortfolioScope = 'own' | 'external' | 'historic'

export interface PortfolioRequirement {
  id: string
  text: string
  completed: boolean
}

export interface PortfolioMilestone {
  id: string
  title: string
  date: string
  completed: boolean
}

export interface PortfolioEntry {
  id: string
  entry_type: PortfolioType
  scope: PortfolioScope
  title: string
  description: string
  institution: string
  url: string
  contact: string
  funding_source: string
  budget: number | null
  principal_investigator: string
  collaborators: string[]
  participants: string[]
  start_date: string
  end_date: string | null
  hours: number | null
  modality: string
  status: PortfolioStatus
  requirements: PortfolioRequirement[]
  milestones: PortfolioMilestone[]
  tags: string[]
  notes: string
  related_files: string[]
  related_inventory: string[]
  created_at: string
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
  end_time: string | null
  location: string | null
  created_by: string
  invitees: string[]
  accepted: string[]
  declined: string[]
  remind_before_min: number
  notify_telegram: boolean
  recurrence: string
  recurrence_end: string | null
  created_at: string
  category: string | null
}

export interface EventCategory {
  id: string
  name: string
  color: string
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
