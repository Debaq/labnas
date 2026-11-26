import { Download } from 'lucide-react'
import type { RecordView } from '../../api'

/** Ficha de solo lectura de un registro compartido (autorizado o publico) */
export default function RecordCard({ record, downloadUrl }: { record: RecordView; downloadUrl?: string }) {
  return (
    <div className="rounded-xl p-6 space-y-4" style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
      <div>
        <p className="text-xs uppercase tracking-wide" style={{ color: 'var(--text-secondary)' }}>{record.kind_label}</p>
        <h2 className="text-xl font-semibold" style={{ color: 'var(--text-primary)' }}>{record.title}</h2>
      </div>
      {record.fields.length > 0 && (
        <dl className="grid grid-cols-1 sm:grid-cols-[max-content_1fr] gap-x-6 gap-y-2 text-sm">
          {record.fields.map(f => (
            <div key={f.label} className="contents">
              <dt style={{ color: 'var(--text-secondary)' }}>{f.label}</dt>
              <dd className="whitespace-pre-line" style={{ color: 'var(--text-primary)' }}>{f.value}</dd>
            </div>
          ))}
        </dl>
      )}
      {record.text && (
        <p className="text-sm whitespace-pre-wrap pt-3" style={{ color: 'var(--text-primary)', borderTop: '1px solid var(--border)' }}>{record.text}</p>
      )}
      {record.download && downloadUrl && (
        <a href={downloadUrl} className="inline-flex items-center gap-2 px-4 py-2 rounded-lg text-sm" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
          <Download size={15} /> Descargar
        </a>
      )}
    </div>
  )
}
