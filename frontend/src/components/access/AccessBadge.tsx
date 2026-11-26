import type { RecordKind, Visibility } from '../../api'
import AccessButton from './AccessButton'
import { VISIBILITY } from './levels'

/**
 * Nivel de acceso en una lista: quien puede gestionarlo ve el boton para cambiarlo; el resto
 * solo ve la insignia, y nada si es privado.
 */
export default function AccessBadge({ kind, id, level, canManage, compact, onChanged }: {
  kind: RecordKind
  id: string
  level: Visibility
  canManage: boolean
  compact?: boolean
  onChanged?: (v: Visibility) => void
}) {
  if (canManage) return <AccessButton kind={kind} id={id} level={level} compact={compact} onChanged={onChanged} />
  if (level === 'privado') return null
  const meta = VISIBILITY[level]
  const Icon = meta.icon
  return (
    <span title={meta.hint} className="inline-flex items-center gap-1 px-2 py-0.5 rounded-md text-xs" style={{ color: meta.color }}>
      <Icon size={13} />{!compact && meta.label}
    </span>
  )
}
