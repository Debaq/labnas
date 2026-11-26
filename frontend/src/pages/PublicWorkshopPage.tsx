import { useEffect, useState } from 'react'
import { Loader2 } from 'lucide-react'
import { fetchPublicWorkshop, enrollPublicWorkshop, type PublicWorkshop, type EnrollResult } from '../api'

const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
const input = { backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)', color: 'var(--text-primary)' }

/** Inscripcion publica a un taller (/taller/<token>): sin cuenta ni menu */
export default function PublicWorkshopPage({ token }: { token: string }) {
  const [w, setW] = useState<PublicWorkshop | null>(null)
  const [missing, setMissing] = useState(false)
  const [name, setName] = useState('')
  const [email, setEmail] = useState('')
  const [affiliation, setAffiliation] = useState('')
  const [result, setResult] = useState<EnrollResult | null>(null)
  const [error, setError] = useState('')
  const [busy, setBusy] = useState(false)

  useEffect(() => { fetchPublicWorkshop(token).then(setW).catch(() => setMissing(true)) }, [token])

  async function submit(e: React.FormEvent) {
    e.preventDefault()
    setBusy(true); setError('')
    try { setResult(await enrollPublicWorkshop(token, { name, email, affiliation })) } catch (err) { setError(err instanceof Error ? err.message : 'Error') }
    setBusy(false)
  }

  return (
    <div className="min-h-screen px-4 py-10" style={{ backgroundColor: 'var(--bg-primary)' }}>
      <div className="max-w-xl mx-auto space-y-5">
        {missing && (
          <div className="rounded-xl p-6" style={card}>
            <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>No encontrado</h1>
            <p className="text-sm mt-1" style={{ color: 'var(--text-secondary)' }}>El enlace no existe o la inscripcion esta cerrada.</p>
          </div>
        )}
        {!w && !missing && <Loader2 className="animate-spin" size={24} style={{ color: 'var(--accent)' }} />}
        {w && (
          <>
            <p className="text-sm font-semibold" style={{ color: 'var(--text-secondary)' }}>{w.lab.lab_name}</p>
            <div className="rounded-xl p-6 space-y-2" style={card}>
              <h1 className="text-xl font-semibold" style={{ color: 'var(--text-primary)' }}>{w.title}</h1>
              {w.description && <p className="text-sm whitespace-pre-wrap" style={{ color: 'var(--text-secondary)' }}>{w.description}</p>}
              <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                {w.start_date}{w.end_date ? ` al ${w.end_date}` : ''}{w.hours ? ` · ${w.hours} horas` : ''}{w.modality ? ` · ${w.modality}` : ''}
              </p>
              {w.sessions.length > 0 && <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Sesiones: {w.sessions.join(', ')}</p>}
              {w.capacity > 0 && <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Cupos: {w.enrolled}/{w.capacity}{w.full ? ' (lleno: quedaras en lista de espera)' : ''}</p>}
            </div>
            {result ? (
              <div className="rounded-xl p-6" style={card}>
                <p className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                  {result.status === 'inscrito' ? 'Listo, quedaste inscrito.' : `Quedaste en lista de espera (lugar ${result.waitlist_position}). Si se libera un cupo, subes automaticamente.`}
                </p>
              </div>
            ) : (
              <form onSubmit={submit} className="rounded-xl p-6 space-y-3" style={card}>
                <input required maxLength={100} placeholder="Nombre completo" className="w-full px-3 py-2 rounded-lg text-sm" style={input} value={name} onChange={e => setName(e.target.value)} />
                <input required type="email" maxLength={120} placeholder="Correo" className="w-full px-3 py-2 rounded-lg text-sm" style={input} value={email} onChange={e => setEmail(e.target.value)} />
                <input maxLength={120} placeholder="Afiliacion o carrera" className="w-full px-3 py-2 rounded-lg text-sm" style={input} value={affiliation} onChange={e => setAffiliation(e.target.value)} />
                {error && <p className="text-xs text-red-500">{error}</p>}
                <button disabled={busy} className="px-4 py-2 rounded-lg text-sm font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Inscribirme</button>
              </form>
            )}
          </>
        )}
      </div>
    </div>
  )
}
