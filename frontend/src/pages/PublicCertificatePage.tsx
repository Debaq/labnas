import { useEffect, useState } from 'react'
import { Loader2, BadgeCheck, BadgeX } from 'lucide-react'
import { fetchPublicCertificate, type Certificate } from '../api'
import { printCertificate } from '../lib/certificate'

/** Verificacion publica de un certificado (/certificado/<codigo>): sin cuenta ni menu */
export default function PublicCertificatePage({ code }: { code: string }) {
  const [c, setC] = useState<Certificate | null>(null)
  const [missing, setMissing] = useState(false)

  useEffect(() => { fetchPublicCertificate(code).then(setC).catch(() => setMissing(true)) }, [code])

  const card = { backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }
  return (
    <div className="min-h-screen px-4 py-10" style={{ backgroundColor: 'var(--bg-primary)' }}>
      <div className="max-w-xl mx-auto space-y-5">
        {!c && !missing && <Loader2 className="animate-spin" size={24} style={{ color: 'var(--accent)' }} />}
        {missing && (
          <div className="rounded-xl p-6 flex items-center gap-3" style={card}>
            <BadgeX size={28} className="text-red-500" />
            <div>
              <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>Certificado no encontrado</h1>
              <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>El codigo no corresponde a ningun certificado emitido.</p>
            </div>
          </div>
        )}
        {c && (
          <>
            <p className="text-sm font-semibold" style={{ color: 'var(--text-secondary)' }}>{c.lab.lab_name}{c.lab.institution ? ` - ${c.lab.institution}` : ''}</p>
            <div className="rounded-xl p-6 space-y-2" style={card}>
              <div className="flex items-center gap-2">
                {c.valid ? <BadgeCheck size={26} style={{ color: 'var(--success)' }} /> : <BadgeX size={26} className="text-red-500" />}
                <h1 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>{c.valid ? 'Certificado autentico' : 'Certificado anulado'}</h1>
              </div>
              <p className="text-sm" style={{ color: 'var(--text-primary)' }}><b>{c.name}</b> asistio al taller <b>{c.workshop}</b>.</p>
              <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>
                {c.start_date}{c.end_date ? ` al ${c.end_date}` : ''}{c.hours ? ` · ${c.hours} horas` : ''} · asistencia {c.attended.length} de {c.sessions.length} sesiones
              </p>
              <p className="text-xs" style={{ color: 'var(--text-secondary)' }}>Codigo: {c.code}</p>
              {c.valid && (
                <button className="mt-2 px-3 py-1.5 rounded-lg text-xs font-medium" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}
                  onClick={() => printCertificate(c)}>Imprimir</button>
              )}
            </div>
          </>
        )}
      </div>
    </div>
  )
}
