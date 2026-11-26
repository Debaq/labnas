import { Download } from 'lucide-react'
import { publicReportUrl, type Portal } from '../../api'
import { money } from '../../lib/finance'
import { CATEGORY_LABEL, PROJECT_STATUS, cardStyle, fmtDay, fmtSize } from '../../lib/transparency'

const th = 'text-left text-[11px] font-medium uppercase tracking-wide px-3 py-2'
const td = 'px-3 py-2 text-sm'

function Section({ title, children, empty }: { title: string; children: React.ReactNode; empty?: boolean }) {
  return (
    <section className="rounded-xl overflow-hidden" style={cardStyle}>
      <h2 className="px-4 py-3 text-sm font-semibold" style={{ color: 'var(--text-primary)', borderBottom: '1px solid var(--card-border)' }}>{title}</h2>
      {empty ? <p className="px-4 py-4 text-sm" style={{ color: 'var(--text-secondary)' }}>Todavia no hay datos para mostrar.</p> : <div className="overflow-x-auto">{children}</div>}
    </section>
  )
}

/** El contenido del portal: lo ve el publico y tambien la vista previa del administrador */
export default function PortalView({ portal, preview = false }: { portal: Portal; preview?: boolean }) {
  const text = { color: 'var(--text-primary)' }
  const dim = { color: 'var(--text-secondary)' }
  const has = (s: string) => portal.sections.includes(s as never)
  return (
    <div className="space-y-5">
      {portal.intro && <p className="text-sm whitespace-pre-wrap" style={dim}>{portal.intro}</p>}
      {portal.sections.length === 0 && (
        <p className="text-sm rounded-xl p-4" style={{ ...cardStyle, ...dim }}>
          {preview ? 'No hay secciones activas: el portal solo mostraria el formulario de solicitudes.' : 'Por ahora no hay informacion publicada. Puedes enviar una solicitud de informacion abajo.'}
        </p>
      )}

      {has('proyectos') && (
        <Section title="Proyectos" empty={!portal.proyectos?.length}>
          <table className="w-full">
            <thead><tr style={dim}><th className={th}>Proyecto</th><th className={th}>Institucion</th><th className={th}>Financiamiento</th><th className={th}>Fechas</th><th className={th}>Estado</th></tr></thead>
            <tbody>
              {portal.proyectos?.map((p) => (
                <tr key={p.id} style={{ borderTop: '1px solid var(--card-border)', ...text }}>
                  <td className={`${td} font-medium`}>{p.title}</td>
                  <td className={td}>{p.institution || '—'}</td>
                  <td className={td}>{p.funding_source || '—'}</td>
                  <td className={td}>{fmtDay(p.start_date) || '—'}{p.end_date ? ` al ${fmtDay(p.end_date)}` : ''}</td>
                  <td className={td}>{PROJECT_STATUS[p.status] ?? p.status}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Section>
      )}

      {has('montos') && (
        <Section title="Montos por proyecto" empty={!portal.montos?.length}>
          <div className="divide-y" style={{ borderColor: 'var(--card-border)' }}>
            {portal.montos?.map((m) => (
              <div key={m.project_id} className="p-4 space-y-2">
                <p className="text-sm font-medium" style={text}>{m.title}</p>
                <table className="w-full">
                  <thead><tr style={dim}><th className={th}>Categoria</th><th className={`${th} text-right`}>Aprobado</th><th className={`${th} text-right`}>Comprometido</th><th className={`${th} text-right`}>Gastado</th></tr></thead>
                  <tbody>
                    {m.categories.map((c) => (
                      <tr key={c.category} style={{ borderTop: '1px solid var(--card-border)', ...text }}>
                        <td className={td}>{CATEGORY_LABEL[c.category] ?? c.category}</td>
                        <td className={`${td} text-right`}>{money(c.approved, m.currency)}</td>
                        <td className={`${td} text-right`}>{money(c.committed, m.currency)}</td>
                        <td className={`${td} text-right`}>{money(c.spent, m.currency)}</td>
                      </tr>
                    ))}
                    <tr style={{ borderTop: '1px solid var(--border)', ...text }} className="font-semibold">
                      <td className={td}>Total</td>
                      <td className={`${td} text-right`}>{money(m.approved, m.currency)}</td>
                      <td className={`${td} text-right`}>{money(m.committed, m.currency)}</td>
                      <td className={`${td} text-right`}>{money(m.spent, m.currency)}</td>
                    </tr>
                  </tbody>
                </table>
              </div>
            ))}
          </div>
        </Section>
      )}

      {has('bienes') && (
        <Section title="Bienes adquiridos con proyectos" empty={!portal.bienes?.length}>
          <table className="w-full">
            <thead><tr style={dim}><th className={th}>N° inventario</th><th className={th}>Bien</th><th className={th}>Proyecto</th><th className={`${th} text-right`}>Costo</th></tr></thead>
            <tbody>
              {portal.bienes?.map((b, i) => (
                <tr key={i} style={{ borderTop: '1px solid var(--card-border)', ...text }}>
                  <td className={td}>{b.inventory_number || '—'}</td>
                  <td className={td}>{b.name}</td>
                  <td className={td}>{b.project}</td>
                  <td className={`${td} text-right`}>{money(b.cost)}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Section>
      )}

      {has('uso') && (
        <Section title="Horas de uso de maquinas" empty={!portal.uso?.length}>
          <table className="w-full">
            <thead><tr style={dim}><th className={th}>Mes</th><th className={th}>Maquina</th><th className={`${th} text-right`}>Horas</th><th className={`${th} text-right`}>Sesiones</th></tr></thead>
            <tbody>
              {portal.uso?.map((u, i) => (
                <tr key={i} style={{ borderTop: '1px solid var(--card-border)', ...text }}>
                  <td className={td}>{u.month}</td>
                  <td className={td}>{u.machine}</td>
                  <td className={`${td} text-right`}>{u.hours.toLocaleString('es-CL')}</td>
                  <td className={`${td} text-right`}>{u.sessions}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </Section>
      )}

      {has('informes') && (
        <Section title="Informes" empty={!portal.informes?.length}>
          <ul className="divide-y" style={{ borderColor: 'var(--card-border)' }}>
            {portal.informes?.map((r) => (
              <li key={r.id} className="px-4 py-3 flex items-center gap-3">
                <div className="flex-1 min-w-0">
                  <p className="text-sm font-medium" style={text}>{r.title}</p>
                  <p className="text-xs" style={dim}>{fmtDay(r.date)}{r.project ? ` · ${r.project}` : ''} · {fmtSize(r.size)}</p>
                  <p className="text-[11px] font-mono break-all" style={dim} title="Huella SHA-256 del archivo">SHA-256: {r.sha256}</p>
                </div>
                {preview ? (
                  <span className="text-xs" style={dim}>{r.file_name}</span>
                ) : (
                  <a href={publicReportUrl(r.id)} target="_blank" rel="noreferrer" className="p-2 rounded-lg" style={{ color: 'var(--accent)' }} title="Descargar"><Download size={16} /></a>
                )}
              </li>
            ))}
          </ul>
        </Section>
      )}
    </div>
  )
}
