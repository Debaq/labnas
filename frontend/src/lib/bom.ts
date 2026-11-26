// Lista de materiales (BOM) exportada desde KiCad (idea 35): se lee el CSV, se agrupan las
// lineas por valor y huella, y se cruzan con el Inventario para saber que falta.

export interface BomLine {
  /** Referencias (R1, R2…) */
  refs: string[]
  value: string
  footprint: string
  /** Cantidad por placa */
  qty: number
}

/** Separa una linea CSV respetando comillas ("a, b") y comillas dobles escapadas */
export function splitCsv(line: string, sep: string): string[] {
  const out: string[] = []
  let cur = ''
  let quoted = false
  for (let i = 0; i < line.length; i++) {
    const c = line[i]
    if (quoted) {
      if (c === '"' && line[i + 1] === '"') { cur += '"'; i++ }
      else if (c === '"') quoted = false
      else cur += c
    } else if (c === '"') quoted = true
    else if (c === sep) { out.push(cur.trim()); cur = '' }
    else cur += c
  }
  out.push(cur.trim())
  return out
}

/** Huella corta: "Resistor_SMD:R_0805_2012Metric" -> "0805" (o el nombre sin libreria) */
export function shortFootprint(fp: string): string {
  const name = fp.includes(':') ? fp.split(':').pop()! : fp
  const size = name.match(/(?:^|[_-])(0201|0402|0603|0805|1206|1210|2010|2512)(?:[_-]|$)/)
  if (size) return size[1]
  return name
}

/** Lee el CSV de KiCad (BOM de Eeschema o plugin): columnas Reference/Value/Footprint/Qty */
export function parseBom(text: string): BomLine[] {
  const lines = text.replace(/^\uFEFF/, '').split(/\r?\n/).filter(l => l.trim() !== '')
  if (lines.length === 0) return []
  const sep = (lines[0].match(/;/g)?.length ?? 0) > (lines[0].match(/,/g)?.length ?? 0) ? ';' : ','
  const head = splitCsv(lines[0], sep).map(h => h.toLowerCase().replace(/[^a-z]/g, ''))
  const col = (...names: string[]) => head.findIndex(h => names.includes(h))
  const iRef = col('reference', 'references', 'ref', 'designator', 'refs')
  const iVal = col('value', 'valor', 'comment')
  const iFp = col('footprint', 'huella', 'package')
  const iQty = col('qty', 'quantity', 'cantidad', 'qnty')
  if (iVal < 0) throw new Error('El CSV no tiene columna Value (exporta la BOM desde KiCad como CSV)')
  const groups = new Map<string, BomLine>()
  for (const raw of lines.slice(1)) {
    const cells = splitCsv(raw, sep)
    const value = cells[iVal] ?? ''
    if (!value) continue
    const footprint = iFp >= 0 ? cells[iFp] ?? '' : ''
    const refs = iRef >= 0 ? (cells[iRef] ?? '').split(/[\s,]+/).filter(Boolean) : []
    const qtyCell = iQty >= 0 ? Number(cells[iQty]) : NaN
    const qty = Number.isFinite(qtyCell) && qtyCell > 0 ? qtyCell : Math.max(1, refs.length)
    const key = `${value.toLowerCase()}|${shortFootprint(footprint).toLowerCase()}`
    const g = groups.get(key)
    if (g) { g.qty += qty; g.refs.push(...refs) }
    else groups.set(key, { refs, value, footprint, qty })
  }
  return [...groups.values()]
}

/** ¿El item del inventario corresponde a la linea? (valor y, si se sabe, el tamaño de la huella) */
export function matches(line: BomLine, itemText: string): boolean {
  const t = itemText.toLowerCase()
  const v = line.value.toLowerCase()
  const fp = shortFootprint(line.footprint).toLowerCase()
  const valueOk = new RegExp(`(^|[^a-z0-9.])${v.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}([^a-z0-9.]|$)`).test(t)
  if (!valueOk) return false
  return !/^\d{4}$/.test(fp) || t.includes(fp)
}
