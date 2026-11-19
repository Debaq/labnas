import { useCallback, useEffect, useState } from 'react'
import { Circle } from 'lucide-react'
import { fetchRecordingNow } from '../api'
import type { RecordingNow } from '../types'
import { useEvent } from '../events/useEvents'

/** Aviso para todos los usuarios mientras alguna camara graba (transparencia) */
export default function RecordingBanner() {
  const [list, setList] = useState<RecordingNow[]>([])
  const load = useCallback(() => { fetchRecordingNow().then(setList).catch(() => {}) }, [])
  useEffect(() => { load() }, [load])
  useEvent<RecordingNow[]>('cameras.recording', (d) => setList(Array.isArray(d) ? d : []), load)

  if (list.length === 0) return null
  return (
    <div className="px-4 py-1.5 text-xs font-medium flex items-center gap-2 flex-wrap" style={{ backgroundColor: 'var(--danger)', color: '#fff' }}>
      <Circle size={9} fill="#fff" className="animate-pulse" />
      {list.map((r) => `Grabando: ${r.camera} (por ${r.by})`).join(' · ')}
    </div>
  )
}
