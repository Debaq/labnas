import { useEffect, useState } from 'react'
import { useSearchParams } from 'react-router-dom'
import { ArrowLeft, Handshake, Loader2 } from 'lucide-react'
import { fetchShared, fetchSharedRecord, sharedDownloadUrl, type RecordKind, type RecordView, type SharedItem } from '../api'
import RecordCard from '../components/access/RecordCard'
import { errorMessage } from '../lib/errors'

/** Lo que otras personas me autorizaron a ver, con su fecha de termino */
export default function SharedPage() {
  const [items, setItems] = useState<SharedItem[] | null>(null)
  const [params, setParams] = useSearchParams()
  const kind = params.get('kind') as RecordKind | null
  const id = params.get('id')
  const [record, setRecord] = useState<RecordView | null>(null)
  const [error, setError] = useState<string | null>(null)

  useEffect(() => { fetchShared().then(setItems).catch(e => setError(errorMessage(e))) }, [])
  useEffect(() => {
    if (!kind || !id) return
    let alive = true
    fetchSharedRecord(kind, id)
      .then(r => { if (alive) { setRecord(r); setError(null) } })
      .catch(e => { if (alive) setError(errorMessage(e)) })
    return () => { alive = false; setRecord(null) }
  }, [kind, id])

  if (kind && id) {
    return (
      <div className="max-w-3xl space-y-4">
        <button onClick={() => setParams({})} className="flex items-center gap-1 text-sm" style={{ color: 'var(--accent)' }}>
          <ArrowLeft size={15} /> Compartido conmigo
        </button>
        {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
        {!record && !error && <Loader2 className="animate-spin" size={22} style={{ color: 'var(--accent)' }} />}
        {record && <RecordCard record={record} downloadUrl={sharedDownloadUrl(kind, id)} />}
      </div>
    )
  }

  return (
    <div className="max-w-3xl space-y-4">
      <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
        Registros y archivos que otras personas te autorizaron a ver. Cada autorizacion tiene fecha de termino y cada vista queda registrada.
      </p>
      {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
      {!items && !error && <Loader2 className="animate-spin" size={22} style={{ color: 'var(--accent)' }} />}
      {items?.length === 0 && (
        <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>No tienes nada compartido vigente.</p>
      )}
      <div className="space-y-2">
        {items?.map(it => (
          <button
            key={`${it.kind}:${it.id}`}
            onClick={() => setParams({ kind: it.kind, id: it.id })}
            className="w-full flex items-center justify-between gap-3 p-4 rounded-xl text-left"
            style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
          >
            <span className="flex items-center gap-3 min-w-0">
              <Handshake size={18} style={{ color: 'var(--warning)' }} />
              <span className="min-w-0">
                <span className="block text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>{it.title}</span>
                <span className="block text-xs" style={{ color: 'var(--text-secondary)' }}>{it.kind_label} · de {it.granted_by}</span>
              </span>
            </span>
            <span className="text-xs shrink-0" style={{ color: 'var(--text-secondary)' }}>
              hasta {new Date(it.expires_at * 1000).toLocaleDateString('es-CL')}
            </span>
          </button>
        ))}
      </div>
    </div>
  )
}
