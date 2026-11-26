import { useEffect, useState } from 'react'
import { Loader2, Search, Send } from 'lucide-react'
import { fetchPublicPortal, lookupInfoRequest, submitInfoRequest, type Portal, type RequestLookup } from '../api'
import PortalView from '../components/transparency/PortalView'
import { errorMessage } from '../lib/errors'
import { REQUEST_STATUS, cardStyle, deadlineColor, deadlineLabel, fmtDay, inputStyle } from '../lib/transparency'

const text = { color: 'var(--text-primary)' }
const dim = { color: 'var(--text-secondary)' }
const field = 'block w-full mt-1 px-3 py-2 rounded-lg text-sm outline-none'

function NewRequest({ days }: { days: number }) {
  const [name, setName] = useState('')
  const [email, setEmail] = useState('')
  const [body, setBody] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [done, setDone] = useState<{ folio: string; code: string; due_date: string } | null>(null)

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    setBusy(true); setError('')
    try { setDone(await submitInfoRequest({ name, email, body })) } catch (err) { setError(errorMessage(err)) }
    setBusy(false)
  }

  if (done) {
    return (
      <div className="rounded-xl p-5 space-y-3" style={cardStyle}>
        <p className="text-sm font-semibold" style={text}>Recibimos tu solicitud</p>
        <div className="grid gap-3 sm:grid-cols-2">
          <div className="rounded-lg p-3" style={{ border: '1px solid var(--border)' }}><p className="text-xs" style={dim}>Folio</p><p className="font-mono text-lg" style={text}>{done.folio}</p></div>
          <div className="rounded-lg p-3" style={{ border: '1px solid var(--border)' }}><p className="text-xs" style={dim}>Codigo secreto</p><p className="font-mono text-lg tracking-wider" style={text}>{done.code}</p></div>
        </div>
        <p className="text-sm" style={{ color: 'var(--danger)' }}>Anota o guarda el folio y el codigo ahora: el codigo no se puede recuperar. Con los dos consultas el estado y la respuesta.</p>
        <p className="text-xs" style={dim}>Plazo de respuesta: hasta el {fmtDay(done.due_date)}.</p>
      </div>
    )
  }
  return (
    <form onSubmit={submit} className="rounded-xl p-5 space-y-3" style={cardStyle}>
      <h2 className="text-sm font-semibold" style={text}>Solicitar informacion</h2>
      <p className="text-xs" style={dim}>Respondemos en un plazo de {days} dias habiles. Te damos un folio y un codigo para seguir tu solicitud aqui mismo.</p>
      <label className="block text-xs" style={dim}>Nombre
        <input required maxLength={100} value={name} onChange={(e) => setName(e.target.value)} className={field} style={inputStyle} />
      </label>
      <label className="block text-xs" style={dim}>Correo (opcional; solo para contactarte si hace falta)
        <input type="email" maxLength={120} value={email} onChange={(e) => setEmail(e.target.value)} className={field} style={inputStyle} />
      </label>
      <label className="block text-xs" style={dim}>Que informacion necesitas
        <textarea required minLength={10} maxLength={4000} rows={5} value={body} onChange={(e) => setBody(e.target.value)} className={field} style={inputStyle} />
      </label>
      {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
      <button disabled={busy} className="inline-flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
        {busy ? <Loader2 size={14} className="animate-spin" /> : <Send size={14} />} Enviar solicitud
      </button>
    </form>
  )
}

function Lookup() {
  const [folio, setFolio] = useState('')
  const [code, setCode] = useState('')
  const [busy, setBusy] = useState(false)
  const [error, setError] = useState('')
  const [found, setFound] = useState<RequestLookup | null>(null)

  const submit = async (e: React.FormEvent) => {
    e.preventDefault()
    setBusy(true); setError(''); setFound(null)
    try { setFound(await lookupInfoRequest({ folio, code })) } catch (err) { setError(errorMessage(err)) }
    setBusy(false)
  }

  return (
    <div className="rounded-xl p-5 space-y-3" style={cardStyle}>
      <h2 className="text-sm font-semibold" style={text}>Consultar una solicitud</h2>
      <form onSubmit={submit} className="grid gap-3 sm:grid-cols-[1fr_1fr_auto] items-end">
        <label className="block text-xs" style={dim}>Folio
          <input required placeholder="SI-2026-0001" maxLength={24} value={folio} onChange={(e) => setFolio(e.target.value)} className={`${field} font-mono`} style={inputStyle} />
        </label>
        <label className="block text-xs" style={dim}>Codigo
          <input required maxLength={16} autoComplete="off" value={code} onChange={(e) => setCode(e.target.value)} className={`${field} font-mono`} style={inputStyle} />
        </label>
        <button disabled={busy} className="inline-flex items-center justify-center gap-2 px-4 py-2 rounded-lg text-sm font-medium disabled:opacity-50" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
          {busy ? <Loader2 size={14} className="animate-spin" /> : <Search size={14} />} Consultar
        </button>
      </form>
      {error && <p className="text-sm" style={{ color: 'var(--danger)' }}>{error}</p>}
      {found && (
        <div className="rounded-lg p-4 space-y-2" style={{ border: '1px solid var(--border)' }}>
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-mono text-sm" style={text}>{found.folio}</span>
            <span className="text-[11px] px-2 py-0.5 rounded-full" style={{ color: REQUEST_STATUS[found.status].color, border: '1px solid currentColor' }}>{REQUEST_STATUS[found.status].label}</span>
            {found.days_left != null && <span className="text-[11px]" style={{ color: deadlineColor(found.days_left) }}>{deadlineLabel(found.days_left)}</span>}
          </div>
          <p className="text-xs" style={dim}>Recibida el {fmtDay(found.created_at)} · plazo hasta el {fmtDay(found.due_date)}</p>
          <p className="text-sm whitespace-pre-wrap" style={dim}>{found.body}</p>
          {found.response && <div className="rounded-lg p-3" style={{ backgroundColor: 'var(--bg-secondary)' }}><p className="text-xs mb-1" style={dim}>Respuesta</p><p className="text-sm whitespace-pre-wrap" style={text}>{found.response}</p></div>}
          {found.status === 'rechazada' && found.reason && <p className="text-sm" style={text}>Motivo: {found.reason}</p>}
          {found.status === 'derivada' && found.derived_to && <p className="text-sm" style={text}>Derivada a: {found.derived_to}</p>}
          <ul className="text-xs space-y-0.5" style={dim}>
            {found.history.map((h, i) => <li key={i}>{fmtDay(h.at)} · {REQUEST_STATUS[h.status]?.label ?? h.status}</li>)}
          </ul>
        </div>
      )}
    </div>
  )
}

/** Portal publico de transparencia (/portal-transparencia): sin cuenta; con el modulo apagado no existe */
export default function PublicTransparencyPage() {
  const [portal, setPortal] = useState<Portal | null>(null)
  const [missing, setMissing] = useState(false)

  useEffect(() => { fetchPublicPortal().then(setPortal).catch(() => setMissing(true)) }, [])

  return (
    <div className="min-h-screen px-4 py-10" style={{ backgroundColor: 'var(--bg-primary)' }}>
      <div className="max-w-4xl mx-auto space-y-6">
        {missing && (
          <div className="rounded-xl p-6" style={cardStyle}>
            <h1 className="text-lg font-semibold" style={text}>No encontrado</h1>
            <p className="text-sm mt-1" style={dim}>Esta pagina no esta disponible.</p>
          </div>
        )}
        {!portal && !missing && <Loader2 className="animate-spin" size={24} style={{ color: 'var(--accent)' }} />}
        {portal && (
          <>
            <header className="flex items-center gap-4">
              {portal.lab.logo_url && <img src={portal.lab.logo_url} alt="" className="h-12 w-12 object-contain" />}
              <div>
                <p className="text-sm" style={dim}>{portal.lab.institution}</p>
                <h1 className="text-2xl font-semibold" style={text}>{portal.lab.lab_name}</h1>
                <p className="text-sm" style={dim}>Portal de transparencia</p>
              </div>
            </header>
            <PortalView portal={portal} />
            <p className="text-xs" style={dim}>Generado automaticamente desde los datos del laboratorio el {fmtDay(portal.generated_at)}. Los montos estan en pesos chilenos.</p>
            <NewRequest days={portal.deadline_days} />
            <Lookup />
          </>
        )}
      </div>
    </div>
  )
}
