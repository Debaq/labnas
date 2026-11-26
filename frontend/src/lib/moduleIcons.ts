import { Box, ClipboardList, Eye, FileText, FolderOpen, GraduationCap, Hourglass, Landmark, LayoutDashboard, Mail, Music, Network, Package, Printer, ShieldAlert, TerminalSquare, Thermometer, Video, Wrench } from 'lucide-react'
import type { IconComponent } from './icons'

/** Icono de cada modulo por su nombre en el registro del servidor (backend/src/modules.rs) */
const ICONS: Record<string, IconComponent> = {
  Box, ClipboardList, Eye, FileText, FolderOpen, GraduationCap, Hourglass, Landmark, LayoutDashboard, Mail, Music,
  Network, Package, Printer, ShieldAlert, TerminalSquare, Thermometer, Video, Wrench,
}

export function moduleIcon(name?: string): IconComponent {
  return (name && ICONS[name]) || Box
}
