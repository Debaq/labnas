import { Fragment, useCallback, useEffect, useState } from 'react'
import { ChevronDown, ChevronRight, Loader2, Pencil, Plus, Trash2 } from 'lucide-react'
import { deleteBudgetItem, fetchExecution, fetchExecutionSummary, type ItemExec, type ProjectExec, type SummaryRow } from '../../../api'
import { useEvent } from '../../../events/useEvents'
import { useToast } from '../../ToastContext'
import { errorMessage } from '../../../lib/errors'
import { money, type FinanceTabProps } from '../../../lib/finance'
import { CATEGORY_LABEL, LEVEL, cardStyle } from '../../../lib/financeBudget'
import Bar from './Bar'
import ConfigPanel from './ConfigPanel'
import ItemDetailRow from './ItemDetailRow'
import ItemForm from './ItemForm'

const TH = 'text-left py-2 px-2 font-medium text-xs'

function Summary({ rows }: { rows: SummaryRow[] }) {
  if (rows.length === 0) {
    return <p className="text-sm text-center py-10" style={{ color: 'var(--text-secondary)' }}>Aún no hay presupuestos. Elige un proyecto arriba para armar el suyo.</p>
  }
  return (
    <div className="rounded-xl overflow-x-auto" style={cardStyle}>
      <table className="w-full text-sm" style={{ color: 'var(--text-primary)' }}>
        <thead><tr style={{ borderBottom: '1px solid var(--border)', color: 'var(--text-secondary)' }}>
          <th className={TH}>Proyecto</th><th className={TH}>Aprobado</th><th className={TH}>Comprometido</th><th className={TH}>Gastado</th><th className={TH}>Disponible</th><th className={TH}>Avance</th><th className={TH}>Estado</th>
        </tr></thead>
        <tbody>
          {rows.map((r) => (
            <tr key={r.project_id} style={{ borderBottom: '1px solid var(--border)' }}>
              <td className="py-2 px-2">{r.title}{r.closed_at && <span className="text-xs ml-1" style={{ color: 'var(--text-secondary)' }}>(cerrado)</span>}</td>
              <td className="px-2 font-mono">{money(r.totals.approved, r.currency)}</td>
              <td className="px-2 font-mono">{money(r.totals.committed, r.currency)}</td>
              <td className="px-2 font-mono">{money(r.totals.spent, r.currency)}</td>
              <td className="px-2 font-mono" style={{ color: r.totals.available < 0 ? 'var(--danger)' : undefined }}>{money(r.totals.available, r.currency)}</td>
              <td className="px-2"><Bar pct={r.totals.pct} alertPct={80} /></td>
              <td className="px-2 text-xs" style={{ color: LEVEL[r.level].color }}>{LEVEL[r.level].label}</td>
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  )
}

/** Presupuesto por ítem y ejecución en vivo (aprobado, comprometido, gastado, disponible) */
export default function ExecutionPanel({ projectId, staff }: FinanceTabProps) {
  const { addToast } = useToast()
  const [summary, setSummary] = useState<SummaryRow[] | null>(null)
  const [exec, setExec] = useState<ProjectExec | null>(null)
  const [editing, setEditing] = useState<ItemExec | 'new' | null>(null)
  const [openItem, setOpenItem] = useState<string | null>(null)
  const [version, setVersion] = useState(0)

  const load = useCallback(() => {
    if (projectId) fetchExecution(projectId).then((x) => { setExec(x); setVersion((v) => v + 1) }).catch((e) => addToast(errorMessage(e), 'error'))
    else fetchExecutionSummary().then(setSummary).catch((e) => addToast(errorMessage(e), 'error'))
  }, [projectId, addToast])
  useEffect(load, [load])
  useEvent('finance', load, load)

  if (!projectId) return summary ? <Summary rows={summary} /> : <div className="flex justify-center py-10"><Loader2 className="animate-spin" /></div>
  if (!exec) return <div className="flex justify-center py-10"><Loader2 className="animate-spin" /></div>

  const { config, items, by_category: cats, totals, unassigned } = exec
  const cur = config.currency
  const closed = config.closed_at != null
  const canEdit = staff && !closed

  const remove = async (i: ItemExec) => {
    if (!window.confirm(`¿Borrar el ítem «${i.name}»? Queda registrado en la bitácora.`)) return
    try {
      await deleteBudgetItem(i.id)
      addToast('Ítem eliminado', 'success')
      load()
    } catch (e) {
      addToast(errorMessage(e), 'error')
    }
  }

  return (
    <div className="space-y-4">
      <ConfigPanel key={config.project_id + (config.closed_at ?? '') + config.alert_pct + config.responsible} config={config} staff={staff} onSaved={load} />

      <div className="grid gap-3 grid-cols-2 md:grid-cols-4">
        {[
          ['Aprobado', totals.approved],
          ['Comprometido', totals.committed],
          ['Gastado', totals.spent],
          ['Disponible', totals.available],
        ].map(([label, v]) => (
          <div key={label as string} className="rounded-xl p-3" style={cardStyle}>
            <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>{label}</div>
            <div className="text-lg font-mono" style={{ color: label === 'Disponible' && (v as number) < 0 ? 'var(--danger)' : 'var(--text-primary)' }}>{money(v as number, cur)}</div>
          </div>
        ))}
      </div>
      <Bar pct={totals.pct} alertPct={config.alert_pct} />

      {(unassigned.committed > 0 || unassigned.spent > 0) && (
        <p className="text-xs" style={{ color: 'var(--warning, #d97706)' }}>
          Hay {money(unassigned.committed, cur)} comprometidos y {money(unassigned.spent, cur)} gastados sin ítem de presupuesto asignado.
        </p>
      )}

      {canEdit && editing === null && (
        <button onClick={() => setEditing('new')} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-sm text-white" style={{ backgroundColor: 'var(--accent)' }}>
          <Plus size={14} /> Nuevo ítem
        </button>
      )}
      {editing && (
        <ItemForm key={editing === 'new' ? 'new' : editing.id} projectId={config.project_id} item={editing === 'new' ? null : editing}
          onDone={() => { setEditing(null); load() }} onCancel={() => setEditing(null)} />
      )}

      {items.length === 0 ? (
        <p className="text-sm text-center py-8" style={{ color: 'var(--text-secondary)' }}>Este proyecto aún no tiene ítems de presupuesto.</p>
      ) : (
        <div className="rounded-xl overflow-x-auto" style={cardStyle}>
          <table className="w-full text-sm" style={{ color: 'var(--text-primary)' }}>
            <thead><tr style={{ borderBottom: '1px solid var(--border)', color: 'var(--text-secondary)' }}>
              <th className={TH}></th><th className={TH}>Ítem</th><th className={TH}>Categoría</th><th className={TH}>Aprobado</th><th className={TH}>Comprometido</th><th className={TH}>Gastado</th><th className={TH}>Disponible</th><th className={TH}>Avance</th><th className={TH}></th>
            </tr></thead>
            <tbody>
              {items.map((i) => (
                <Fragment key={i.id}>
                  <tr style={{ borderBottom: '1px solid var(--border)' }}>
                    <td className="px-1">
                      <button onClick={() => setOpenItem(openItem === i.id ? null : i.id)} aria-label="Ver detalle" className="p-1">
                        {openItem === i.id ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
                      </button>
                    </td>
                    <td className="py-2 px-2">{i.name}{i.notes && <div className="text-xs" style={{ color: 'var(--text-secondary)' }}>{i.notes}</div>}</td>
                    <td className="px-2 text-xs">{CATEGORY_LABEL[i.category]}</td>
                    <td className="px-2 font-mono">{money(i.approved, cur)}</td>
                    <td className="px-2 font-mono">{money(i.committed, cur)}</td>
                    <td className="px-2 font-mono">{money(i.spent, cur)}</td>
                    <td className="px-2 font-mono" style={{ color: i.available < 0 ? 'var(--danger)' : undefined }}>{money(i.available, cur)}</td>
                    <td className="px-2"><Bar pct={i.pct} alertPct={config.alert_pct} /></td>
                    <td className="px-2 whitespace-nowrap">
                      {canEdit && (
                        <>
                          <button onClick={() => setEditing(i)} aria-label="Editar" className="p-1"><Pencil size={14} /></button>
                          <button onClick={() => remove(i)} aria-label="Borrar" className="p-1" style={{ color: 'var(--danger)' }}><Trash2 size={14} /></button>
                        </>
                      )}
                    </td>
                  </tr>
                  {openItem === i.id && (
                    <tr style={{ borderBottom: '1px solid var(--border)' }}><td colSpan={9} className="p-3"><ItemDetailRow itemId={i.id} currency={cur} version={version} /></td></tr>
                  )}
                </Fragment>
              ))}
            </tbody>
          </table>
        </div>
      )}

      {cats.length > 0 && (
        <div className="rounded-xl p-4 space-y-2" style={cardStyle}>
          <h3 className="text-sm font-semibold" style={{ color: 'var(--text-primary)' }}>Por categoría</h3>
          {cats.map((c) => (
            <div key={c.category} className="grid grid-cols-[7rem_1fr_auto] gap-3 items-center text-xs" style={{ color: 'var(--text-primary)' }}>
              <span>{CATEGORY_LABEL[c.category]}</span>
              <Bar pct={c.pct} alertPct={config.alert_pct} />
              <span className="font-mono" title="Aprobado / disponible">
                {money(c.approved, cur)} · libre {money(c.available, cur)}
              </span>
            </div>
          ))}
        </div>
      )}
    </div>
  )
}
