/** Metadatos que los slicers escriben como comentarios en el gcode */
export interface GcodeInfo {
  /** Filamento en gramos (si el slicer lo informa) */
  grams?: number
  /** Filamento en metros */
  meters?: number
  /** Tiempo estimado de impresion */
  seconds?: number
  slicer?: string
}

/** Bytes que se leen del inicio y del final (Cura escribe arriba; Prusa/Orca/Bambu abajo) */
const CHUNK = 256 * 1024

const sum = (list: string) =>
  list.split(',').map((x) => parseFloat(x.trim())).filter((n) => !isNaN(n)).reduce((a, b) => a + b, 0)

/** "1d 2h 3m 4s" / "2h 5m" / "45m 10s" -> segundos */
export function parseDuration(text: string): number | undefined {
  const units: Record<string, number> = { d: 86400, h: 3600, m: 60, s: 1 }
  let total = 0
  let found = false
  for (const m of text.matchAll(/(\d+(?:\.\d+)?)\s*([dhms])/gi)) {
    total += parseFloat(m[1]) * units[m[2].toLowerCase()]
    found = true
  }
  return found ? Math.round(total) : undefined
}

export function parseGcodeText(text: string): GcodeInfo {
  const info: GcodeInfo = {}
  const first = (re: RegExp) => text.match(re)?.[1]

  // gramos: Orca/Bambu "total filament weight/used [g]", Prusa "filament used [g]" (multi-extrusor: lista)
  const g = first(/;\s*total filament (?:weight|used) \[g\]\s*[:=]\s*([\d.,\s]+)/i)
    ?? first(/;\s*filament used \[g\]\s*=\s*([\d.,\s]+)/i)
  if (g) info.grams = sum(g)

  // metros: Prusa/Orca "[mm]", Cura ";Filament used: 1.23m"
  const mm = first(/;\s*(?:total )?filament used \[mm\]\s*=\s*([\d.,\s]+)/i)
  const curaM = first(/;\s*Filament used:\s*([\d.,\s]+)m/i)
  if (mm) info.meters = sum(mm) / 1000
  else if (curaM) info.meters = sum(curaM)

  // tiempo: Cura ";TIME:3723"; Orca/Bambu "total estimated time"; Prusa "estimated printing time (normal mode)"
  const curaT = first(/;TIME:(\d+)/)
  const total = first(/;\s*total estimated time\s*[:=]\s*([^\n;]+)/i)
  const prusa = first(/;\s*estimated printing time \(normal mode\)\s*=\s*([^\n]+)/i)
  if (curaT) info.seconds = parseInt(curaT)
  else if (total) info.seconds = parseDuration(total)
  else if (prusa) info.seconds = parseDuration(prusa)

  const slicer = first(/;\s*generated (?:by|with)\s+([^\n]+)/i) ?? first(/;\s*(BambuStudio[^\n]*)/i) ?? first(/;FLAVOR:(\w+)/)
  if (slicer) info.slicer = slicer.trim()
  return info
}

export async function readGcodeInfo(file: File): Promise<GcodeInfo> {
  const head = await file.slice(0, CHUNK).text()
  const tail = file.size > CHUNK ? await file.slice(Math.max(CHUNK, file.size - CHUNK)).text() : ''
  return parseGcodeText(head + '\n' + tail)
}

/** Gramos a partir de metros de filamento (diametro en mm, densidad en g/cm3) */
export function gramsFromMeters(meters: number, density: number, diameterMm = 1.75): number {
  const radiusCm = diameterMm / 20
  return Math.PI * radiusCm * radiusCm * meters * 100 * density
}
