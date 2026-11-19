import { useCallback, useEffect, useState } from 'react'
import { ScanEye } from 'lucide-react'
import { fetchDetectingNow, type DetectingNow } from '../api'
import { useEvent } from '../events/useEvents'

/** Aviso para todos los usuarios mientras una camara esta vigilando con IA (transparencia) */
export default function DetectionBanner() {
  const [list, setList] = useState<DetectingNow[]>([])
  const load = useCallback(() => { fetchDetectingNow().then(setList).catch(() => {}) }, [])
  useEffect(() => { load() }, [load])
  useEvent<DetectingNow[]>('cameras.detecting', (d) => setList(Array.isArray(d) ? d : []), load)

  if (list.length === 0) return null
  return (
    <div className="px-4 py-1.5 text-xs font-medium flex items-center gap-2 flex-wrap" style={{ backgroundColor: 'var(--warning)', color: '#1a1a1a' }}>
      <ScanEye size={13} />
      {list.map((d) => `Detección activa: ${d.camera} (${d.kinds.join(', ')})`).join(' · ')}
    </div>
  )
}
