import { useMemo, useState } from 'react'
import { CircuitBoard, Download, Upload } from 'lucide-react'
import type { InventoryItem } from '../../types'
import { matches, parseBom, shortFootprint, type BomLine } from '../../lib/bom'

const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
const input = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

function itemText(i: InventoryItem) {
  return [i.name, i.model_name, i.description, ...Object.values(i.custom_fields || {})].join(' ')
}

/** Lista de materiales de KiCad contra el Inventario: que hay y que falta comprar */
export default function BomPanel({ items }: { items: InventoryItem[] }) {
  const [bom, setBom] = useState<BomLine[]>([])
  const [file, setFile] = useState('')
  const [boards, setBoards] = useState(1)
  const [error, setError] = useState<string | null>(null)

  const rows = useMemo(() => bom.map(line => {
    const found = items.filter(i => i.status !== 'retirado' && matches(line, itemText(i)))
    const available = found.reduce((s, i) => s + i.quantity - (i.on_loan ?? 0), 0)
    const need = line.qty * boards
    return { line, found, available, need, missing: Math.max(0, need - available) }
  }), [bom, items, boards])

  async function load(f: File) {
    setError(null)
    try {
      setBom(parseBom(await f.text()))
      setFile(f.name)
    } catch (e) {
      setBom([])
      setError((e as Error).message)
    }
  }

  function shoppingList() {
    const esc = (s: string) => `"${s.replace(/"/g, '""').replace(/^[=+\-@]/, "'$&")}"`
    const lines = ['Valor,Huella,Referencias,Necesarias,En inventario,Comprar']
    for (const r of rows.filter(r => r.missing > 0)) {
      lines.push([esc(r.line.value), esc(shortFootprint(r.line.footprint)), esc(r.line.refs.join(' ')), r.need, r.available, r.missing].join(','))
    }
    const blob = new Blob(['\uFEFF' + lines.join('\n')], { type: 'text/csv;charset=utf-8' })
    const a = document.createElement('a')
    a.href = URL.createObjectURL(blob)
    a.download = `compra-${file.replace(/\.[^.]+$/, '') || 'bom'}.csv`
    a.click()
    URL.revokeObjectURL(a.href)
  }

  const missing = rows.filter(r => r.missing > 0).length

  return (
    <div className="space-y-3">
      <div className="rounded-xl p-4 space-y-2" style={card}>
        <h2 className="text-sm font-semibold flex items-center gap-2" style={{ color: 'var(--text-primary)' }}><CircuitBoard size={16} /> Lista de materiales de KiCad</h2>
        <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
          En KiCad (Esquema → Herramientas → Generar lista de materiales) exporta el CSV y subelo aqui. Se cruza cada valor y huella con el Inventario (por nombre, modelo o descripcion, p. ej. «Resistencia 10k 0805») y se arma la lista de lo que falta comprar.
        </p>
        <div className="flex flex-wrap items-center gap-3">
          <label className="flex items-center gap-2 px-3 py-1.5 rounded-lg text-sm cursor-pointer" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            <Upload size={14} /> {file || 'Subir BOM (.csv)'}
            <input type="file" accept=".csv,text/csv" className="hidden" onChange={e => { const f = e.target.files?.[0]; if (f) load(f) }} />
          </label>
          <label className="flex items-center gap-2 text-sm" style={{ color: 'var(--text-secondary)' }}>
            Placas a armar
            <input type="number" min={1} max={1000} value={boards} onChange={e => setBoards(Math.max(1, Number(e.target.value) || 1))} className="w-20 rounded-lg px-2 py-1 text-sm" style={input} />
          </label>
          {missing > 0 && (
            <button onClick={shoppingList} className="flex items-center gap-1 px-3 py-1.5 rounded-lg text-sm" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}>
              <Download size={14} /> Lista de compra ({missing})
            </button>
          )}
        </div>
        {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
      </div>

      {rows.length > 0 && (
        <div className="rounded-xl overflow-x-auto" style={card}>
          <table className="w-full text-xs">
            <thead>
              <tr style={{ color: 'var(--text-secondary)' }}>
                <th className="text-left px-3 py-2">Valor</th><th className="text-left px-3 py-2">Huella</th><th className="text-left px-3 py-2">Referencias</th>
                <th className="text-right px-3 py-2">Necesarias</th><th className="text-right px-3 py-2">Hay</th><th className="text-left px-3 py-2">En inventario</th>
              </tr>
            </thead>
            <tbody>
              {rows.map((r, i) => (
                <tr key={i} style={{ borderTop: '1px solid var(--border)', color: 'var(--text-primary)' }}>
                  <td className="px-3 py-1.5 font-medium">{r.line.value}</td>
                  <td className="px-3 py-1.5 font-mono">{shortFootprint(r.line.footprint)}</td>
                  <td className="px-3 py-1.5" style={{ color: 'var(--text-secondary)' }}>{r.line.refs.join(' ')}</td>
                  <td className="px-3 py-1.5 text-right">{r.need}</td>
                  <td className="px-3 py-1.5 text-right font-semibold" style={{ color: r.missing > 0 ? 'var(--danger)' : 'var(--success)' }}>{r.available}</td>
                  <td className="px-3 py-1.5" style={{ color: 'var(--text-secondary)' }}>
                    {r.found.length ? r.found.map(f => f.name).join(', ') : 'no encontrado'}{r.missing > 0 && <span style={{ color: 'var(--danger)' }}> · faltan {r.missing}</span>}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </div>
  )
}
