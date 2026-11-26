import { useEffect, useState } from 'react'
import { Loader2 } from 'lucide-react'
import { fetchPublicRecord, getBranding, type RecordView } from '../api'
import RecordCard from '../components/access/RecordCard'

/** Ficha publica (/p/<token>): sin cuenta ni menu */
export default function PublicRecordPage({ token }: { token: string }) {
  const [record, setRecord] = useState<RecordView | null>(null)
  const [missing, setMissing] = useState(false)
  const [lab, setLab] = useState('LabNAS')

  useEffect(() => {
    fetchPublicRecord(token).then(setRecord).catch(() => setMissing(true))
    getBranding().then(b => { if (b.lab_name) setLab(b.lab_name) }).catch(() => {})
  }, [token])

  return (
    <div className="min-h-screen px-4 py-10" style={{ backgroundColor: 'var(--bg-primary)' }}>
      <div className="max-w-3xl mx-auto space-y-6">
        <p className="text-sm font-semibold" style={{ color: 'var(--text-secondary)' }}>{lab}</p>
        {missing && (
          <div className="rounded-xl p-6" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
            <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>No encontrado</h1>
            <p className="text-sm mt-1" style={{ color: 'var(--text-secondary)' }}>El enlace no existe o dejo de ser publico.</p>
          </div>
        )}
        {!record && !missing && <Loader2 className="animate-spin" size={24} style={{ color: 'var(--accent)' }} />}
        {record && <RecordCard record={record} downloadUrl={`/api/public/${encodeURIComponent(token)}/download`} />}
      </div>
    </div>
  )
}
