import { Globe, Handshake, Lock } from 'lucide-react'
import type { Visibility } from '../../api'
import type { IconComponent } from '../../lib/icons'

export const VISIBILITY: Record<Visibility, { label: string; hint: string; icon: IconComponent; color: string }> = {
  privado: { label: 'Privado', hint: 'Lo ve el equipo, segun su rol', icon: Lock, color: 'var(--text-secondary)' },
  autorizado: { label: 'Con autorizacion', hint: 'Ademas, las personas que autorices, hasta una fecha', icon: Handshake, color: 'var(--warning)' },
  publico: { label: 'Publico', hint: 'Cualquiera con el enlace, sin cuenta', icon: Globe, color: 'var(--success)' },
}
