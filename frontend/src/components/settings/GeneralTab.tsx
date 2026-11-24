import { useState } from 'react'
import { Palette, PenLine, Key } from 'lucide-react'
import { useAuth } from '../../auth/AuthContext'
import { useTheme } from '../../themes/ThemeContext'
import { themes, type ThemeName } from '../../themes/themes'
import { renameUser, changePassword } from '../../api'
import { errorMessage } from '../../lib/errors'
import TwoFactorSection from '../TwoFactorSection'
import PasskeysSection from '../PasskeysSection'
function ThemeCard({
  name,
  isSelected,
  onClick,
}: {
  name: ThemeName
  isSelected: boolean
  onClick: () => void
}) {
  const t = themes[name]
  return (
    <button
      onClick={onClick}
      className="rounded-xl p-4 transition-all duration-200 hover:shadow-lg hover:-translate-y-0.5 text-left"
      style={{
        backgroundColor: t['bg-primary'],
        border: isSelected ? `2px solid var(--accent)` : `2px solid ${t['border']}`,
      }}
    >
      <div className="flex items-center gap-2 mb-3">
        <span className="text-sm font-semibold" style={{ color: t['text-primary'] }}>
          {name.charAt(0).toUpperCase() + name.slice(1)}
        </span>
        {isSelected && (
          <span
            className="text-xs px-2 py-0.5 rounded-full font-medium"
            style={{ backgroundColor: t.accent + '30', color: t.accent }}
          >
            Activo
          </span>
        )}
      </div>
      <div className="flex gap-1.5 mb-3">
        <span className="w-6 h-6 rounded-full" style={{ backgroundColor: t['bg-primary'], border: `1px solid ${t.border}` }} />
        <span className="w-6 h-6 rounded-full" style={{ backgroundColor: t['sidebar-bg'], border: `1px solid ${t.border}` }} />
        <span className="w-6 h-6 rounded-full" style={{ backgroundColor: t.accent }} />
        <span className="w-6 h-6 rounded-full" style={{ backgroundColor: t.success }} />
        <span className="w-6 h-6 rounded-full" style={{ backgroundColor: t.danger }} />
      </div>
      <div className="flex gap-1">
        <span className="h-1.5 flex-1 rounded-full" style={{ backgroundColor: t.accent }} />
        <span className="h-1.5 flex-[2] rounded-full" style={{ backgroundColor: t['bg-tertiary'] }} />
        <span className="h-1.5 flex-1 rounded-full" style={{ backgroundColor: t.success }} />
      </div>
    </button>
  )
}

export default function GeneralTab() {
  const { theme, setTheme, themeNames } = useTheme()
  const { user: authUser } = useAuth()
  const [currentPw, setCurrentPw] = useState('')
  const [newPw, setNewPw] = useState('')
  const [pwMsg, setPwMsg] = useState<{ ok: boolean; text: string } | null>(null)
  const [newName, setNewName] = useState('')
  const [renameMsg, setRenameMsg] = useState<{ ok: boolean; text: string } | null>(null)

  return (
    <>
      {/* Appearance */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <Palette size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Apariencia
          </h2>
        </div>
        <div className="grid grid-cols-2 sm:grid-cols-3 lg:grid-cols-5 gap-3">
          {/* Auto theme card */}
          <button
            onClick={() => setTheme('auto')}
            className="rounded-xl p-4 transition-all duration-200 hover:shadow-lg hover:-translate-y-0.5 text-left"
            style={{
              background: 'linear-gradient(135deg, #282a36 50%, #f8fafc 50%)',
              border: theme === 'auto' ? '2px solid var(--accent)' : '2px solid var(--border)',
            }}
          >
            <div className="flex items-center gap-2 mb-3">
              <span className="text-sm font-semibold" style={{ color: '#f8f8f2' }}>
                Automatico
              </span>
              {theme === 'auto' && (
                <span
                  className="text-xs px-2 py-0.5 rounded-full font-medium"
                  style={{ backgroundColor: 'var(--accent)' + '30', color: 'var(--accent)' }}
                >
                  Activo
                </span>
              )}
            </div>
            <p className="text-[10px]" style={{ color: '#6272a4' }}>
              Sigue el tema del sistema
            </p>
          </button>
          {/* Theme cards - solo los temas reales, no "auto" */}
          {themeNames.filter(t => t !== 'auto').map((t) => (
            <ThemeCard
              key={t}
              name={t as ThemeName}
              isSelected={t === theme}
              onClick={() => setTheme(t)}
            />
          ))}
        </div>
      </section>

      {/* Rename user */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <PenLine size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Cambiar Nombre de Usuario
          </h2>
        </div>
        <div
          className="rounded-xl p-6"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <p className="text-xs mb-3" style={{ color: 'var(--text-secondary)' }}>
            Usuario actual: <span className="font-medium" style={{ color: 'var(--text-primary)' }}>{authUser?.username}</span>
          </p>
          <div className="flex items-end gap-3 flex-wrap">
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Nuevo nombre</label>
              <input
                value={newName}
                onChange={e => { setNewName(e.target.value); setRenameMsg(null) }}
                placeholder="nuevo_nombre"
                className="px-3 py-2 rounded-lg text-sm outline-none w-52"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
              />
            </div>
            <button
              onClick={async () => {
                if (!newName.trim()) return
                try {
                  const result = await renameUser(newName.trim())
                  setRenameMsg({ ok: true, text: `Nombre cambiado a ${result.username}` })
                  setNewName('')
                  // Actualizar auth en localStorage
                  const saved = localStorage.getItem('labnas_auth')
                  if (saved) {
                    const parsed = JSON.parse(saved)
                    parsed.username = result.username
                    localStorage.setItem('labnas_auth', JSON.stringify(parsed))
                    window.location.reload()
                  }
                } catch (e) {
                  setRenameMsg({ ok: false, text: errorMessage(e) })
                }
              }}
              disabled={!newName.trim()}
              className="px-4 py-2 rounded-lg text-sm font-medium"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              Cambiar
            </button>
          </div>
          {renameMsg && (
            <p className="text-xs mt-3" style={{ color: renameMsg.ok ? 'var(--success)' : 'var(--danger)' }}>
              {renameMsg.text}
            </p>
          )}
        </div>
      </section>

      {/* Change password */}
      <section>
        <div className="flex items-center gap-3 mb-4">
          <Key size={22} style={{ color: 'var(--accent)' }} />
          <h2 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
            Cambiar Contrasena
          </h2>
        </div>
        <div
          className="rounded-xl p-6"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <div className="flex items-end gap-3 flex-wrap">
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Actual</label>
              <input
                type="password"
                value={currentPw}
                onChange={e => { setCurrentPw(e.target.value); setPwMsg(null) }}
                className="px-3 py-2 rounded-lg text-sm outline-none w-44"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
              />
            </div>
            <div>
              <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Nueva</label>
              <input
                type="password"
                value={newPw}
                onChange={e => { setNewPw(e.target.value); setPwMsg(null) }}
                className="px-3 py-2 rounded-lg text-sm outline-none w-44"
                style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
              />
            </div>
            <button
              onClick={async () => {
                if (!currentPw || !newPw) return
                try {
                  await changePassword(currentPw, newPw)
                  setPwMsg({ ok: true, text: 'Contrasena cambiada' })
                  setCurrentPw('')
                  setNewPw('')
                } catch (e) {
                  setPwMsg({ ok: false, text: errorMessage(e) })
                }
              }}
              disabled={!currentPw || !newPw}
              className="px-4 py-2 rounded-lg text-sm font-medium"
              style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
            >
              Cambiar
            </button>
          </div>
          {pwMsg && (
            <p className="text-xs mt-3" style={{ color: pwMsg.ok ? 'var(--success)' : 'var(--danger)' }}>
              {pwMsg.text}
            </p>
          )}
        </div>
      </section>

      <TwoFactorSection />
      <PasskeysSection />
    </>
  )
}
