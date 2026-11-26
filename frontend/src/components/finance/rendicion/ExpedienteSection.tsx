import { Download } from 'lucide-react'
import { expedienteUrl } from '../../../api'
import type { FinanceTabProps } from '../../../lib/finance'
import { cardStyle } from '../../../lib/financeRendicion'

/** Expediente por proyecto (ZIP con indice, respaldos, huellas y bitacora) */
export default function ExpedienteSection({ projectId, projects }: FinanceTabProps) {
  const title = projects.find((p) => p.id === projectId)?.title
  return (
    <div className="rounded-xl p-4 space-y-3 text-sm" style={{ ...cardStyle, color: 'var(--text-primary)' }}>
      <h2 className="font-semibold">Expediente del proyecto</h2>
      <p style={{ color: 'var(--text-secondary)' }}>
        Un ZIP con el indice (HTML y CSV) de compras, pasos, documentos, recepciones, presupuesto, compromisos y horas; todos los
        respaldos con nombres ordenados; <b>huellas.txt</b> con el SHA-256 de cada archivo; los registros de la bitacora inalterable
        que mencionan el proyecto, y las instrucciones para verificarlo sin LabNAS. Cada descarga queda en la bitacora.
      </p>
      {projectId ? (
        <a href={expedienteUrl(projectId)} className="inline-flex items-center gap-2 px-4 py-2 rounded-lg"
          style={{ backgroundColor: 'var(--accent)', color: 'var(--accent-text, #fff)' }}>
          <Download size={16} /> Descargar expediente de {title ?? 'este proyecto'}
        </a>
      ) : (
        <p style={{ color: 'var(--text-secondary)' }}>Elija un proyecto arriba para descargar su expediente.</p>
      )}
      <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
        Para una auditoria: cree una cuenta con el rol Auditor y fecha de termino en Configuracion &gt; Usuarios. Con nivel Ver puede
        ver y descargar la rendicion y el expediente, sin poder cambiar nada.
      </p>
    </div>
  )
}
