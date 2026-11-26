import { useCallback, useEffect, useState } from 'react'
import { HandHelping, QrCode, Undo2 } from 'lucide-react'
import { createLoan, fetchInventoryQr, fetchLoans, fetchUsernames, returnLoan } from '../../api'
import type { InventoryItem, InventoryLoan } from '../../types'
import { useAuth } from '../../auth/AuthContext'
import { printQrLabel } from '../../lib/qrLabel'
import { dueText } from '../../lib/inventory'
import { errorMessage } from '../../lib/errors'

const input = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

function plusDays(n: number) {
  const d = new Date(Date.now() + n * 86400000)
  return `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, '0')}-${String(d.getDate()).padStart(2, '0')}`
}

/** Prestamos de un item (prestar, devolver) y su etiqueta QR */
export default function ItemLoans({ item, onChange }: { item: InventoryItem; onChange: () => void }) {
  const { user, canModule } = useAuth()
  const staff = canModule('inventory', 'gestionar')
  const [loans, setLoans] = useState<InventoryLoan[]>([])
  const [users, setUsers] = useState<string[]>([])
  const [form, setForm] = useState<{ borrower: string; quantity: number; due_on: string; note: string } | null>(null)
  const [error, setError] = useState<string | null>(null)
  const available = item.quantity - (item.on_loan ?? 0)
  const canLend = staff || (item.loanable && canModule('inventory', 'usar'))

  const load = useCallback(() => { fetchLoans({ open: true, item: item.id }).then(setLoans).catch(() => {}) }, [item.id])
  useEffect(() => { load() }, [load, item.on_loan])
  useEffect(() => { if (staff) fetchUsernames().then(setUsers).catch(() => {}) }, [staff])

  async function lend() {
    if (!form) return
    try {
      await createLoan(item.id, { borrower: form.borrower || undefined, quantity: form.quantity, due_on: form.due_on, note: form.note })
      setForm(null); setError(null); load(); onChange()
    } catch (e) { setError(errorMessage(e)) }
  }

  async function giveBack(l: InventoryLoan) {
    try { await returnLoan(l.id); load(); onChange() } catch (e) { setError(errorMessage(e)) }
  }

  async function qr() {
    try {
      const q = await fetchInventoryQr(item.id)
      printQrLabel(q, item.name, item.location, item.loanable ? 'Escanea para ver o pedir prestado' : 'Escanea para ver la ficha')
    } catch (e) { setError(errorMessage(e)) }
  }

  return (
    <div className="col-span-full space-y-2 pt-2" style={{ borderTop: '1px solid var(--border)' }}>
      {error && <p style={{ color: 'var(--danger)' }}>{error}</p>}
      {loans.map(l => (
        <div key={l.id} className="flex items-center justify-between gap-2 px-2 py-1 rounded" style={{ backgroundColor: 'var(--bg-tertiary)' }}>
          <span style={{ color: 'var(--text-primary)' }}>
            Prestado a <b>{l.borrower}</b> ({l.quantity}) · <span style={{ color: l.days_left < 0 ? 'var(--danger)' : 'var(--text-secondary)' }}>{dueText(l)}</span>
            {l.note && <span style={{ color: 'var(--text-secondary)' }}> · {l.note}</span>}
          </span>
          {(staff || l.borrower === user?.username) && (
            <button onClick={() => giveBack(l)} className="flex items-center gap-1 px-2 py-0.5 rounded" style={{ color: 'var(--accent)' }}>
              <Undo2 size={12} /> {l.borrower === user?.username ? 'Devolver' : 'Recibir'}
            </button>
          )}
        </div>
      ))}
      {form ? (
        <div className="grid grid-cols-2 md:grid-cols-4 gap-2 items-end">
          {staff && (
            <select value={form.borrower} onChange={e => setForm({ ...form, borrower: e.target.value })} className="rounded-lg px-2 py-1.5" style={input}>
              <option value="">Para mi</option>
              {users.map(u => <option key={u} value={u}>{u}</option>)}
            </select>
          )}
          <input type="number" min={1} max={available} step="any" value={form.quantity} onChange={e => setForm({ ...form, quantity: Number(e.target.value) })} className="rounded-lg px-2 py-1.5" style={input} aria-label="Cantidad" />
          <input type="date" min={plusDays(0)} value={form.due_on} onChange={e => setForm({ ...form, due_on: e.target.value })} className="rounded-lg px-2 py-1.5" style={input} aria-label="Devolucion" />
          <input value={form.note} onChange={e => setForm({ ...form, note: e.target.value })} placeholder="Para que (opcional)" className="rounded-lg px-2 py-1.5" style={input} />
          <div className="col-span-full flex justify-end gap-2">
            <button onClick={() => setForm(null)} className="px-3 py-1 rounded-lg" style={{ color: 'var(--text-secondary)' }}>Cancelar</button>
            <button onClick={lend} className="px-3 py-1 rounded-lg" style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>Prestar</button>
          </div>
        </div>
      ) : (
        <div className="flex items-center gap-2">
          {canLend && available > 0 && item.status === 'activo' && (
            <button onClick={() => setForm({ borrower: '', quantity: 1, due_on: plusDays(7), note: '' })} className="flex items-center gap-1 px-2 py-1 rounded-lg" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}>
              <HandHelping size={13} /> {staff ? 'Prestar' : 'Llevarlo prestado'}
            </button>
          )}
          <button onClick={qr} className="flex items-center gap-1 px-2 py-1 rounded-lg" style={{ backgroundColor: 'var(--bg-tertiary)', color: 'var(--text-primary)' }}>
            <QrCode size={13} /> Etiqueta QR
          </button>
          {(item.on_loan ?? 0) > 0 && <span style={{ color: 'var(--text-secondary)' }}>Disponibles: {available} de {item.quantity}</span>}
        </div>
      )}
    </div>
  )
}
