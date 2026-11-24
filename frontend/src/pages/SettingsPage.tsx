import { useEffect, useState, type ReactNode } from 'react'
import { useSearchParams } from 'react-router-dom'
import { Building2, Users, Globe, Music, Shield, Bell, Server, SlidersHorizontal, Archive } from 'lucide-react'
import { useAuth } from '../auth/AuthContext'
import { useViaBridge } from '../lib/bridge'
import type { IconComponent } from '../lib/icons'
import GeneralTab from '../components/settings/GeneralTab'
import LabTab from '../components/settings/LabTab'
import UsersTab from '../components/settings/UsersTab'
import NotificationsTab from '../components/settings/NotificationsTab'
import NetworkTab from '../components/settings/NetworkTab'
import MediaTab from '../components/settings/MediaTab'
import SystemTab from '../components/settings/SystemTab'
import BackupsTab from '../components/settings/BackupsTab'
import AdminTab from '../components/settings/AdminTab'

type TabId = 'general' | 'lab' | 'users' | 'notifications' | 'network' | 'media' | 'system' | 'backups' | 'admin'

const TABS: { id: TabId; label: string; icon: IconComponent; adminOnly: boolean }[] = [
  { id: 'general',       label: 'General',        icon: SlidersHorizontal, adminOnly: false },
  { id: 'lab',           label: 'Laboratorio',    icon: Building2,         adminOnly: true  },
  { id: 'users',         label: 'Usuarios',       icon: Users,             adminOnly: true  },
  { id: 'notifications', label: 'Notificaciones', icon: Bell,              adminOnly: true  },
  { id: 'network',       label: 'Red',            icon: Globe,             adminOnly: true  },
  { id: 'media',         label: 'Multimedia',     icon: Music,             adminOnly: true  },
  { id: 'system',        label: 'Sistema',        icon: Server,            adminOnly: true  },
  { id: 'backups',       label: 'Respaldos',      icon: Archive,           adminOnly: true  },
  { id: 'admin',         label: 'Administracion', icon: Shield,            adminOnly: true  },
]

export default function SettingsPage() {
  const { user: authUser } = useAuth()
  const [activeTab, setActiveTab] = useState<TabId>(() => {
    const saved = localStorage.getItem('labnas-settings-tab') as TabId | null
    return saved && TABS.some(t => t.id === saved) ? saved : 'general'
  })
  // Cada pestaña se monta la primera vez que se abre y despues solo se oculta,
  // asi no se pierde lo que se estaba escribiendo al cambiar de pestaña
  const [visited, setVisited] = useState<Set<TabId>>(() => new Set([activeTab]))
  const openTab = (id: TabId) => {
    setActiveTab(id)
    setVisited(v => (v.has(id) ? v : new Set(v).add(id)))
  }
  // Enlaces directos (p.ej. el aviso de actualizacion): ?tab=system&seccion=actualizar
  const [searchParams, setSearchParams] = useSearchParams()
  useEffect(() => {
    const tab = searchParams.get('tab') as TabId | null
    const section = searchParams.get('seccion')
    if (!tab && !section) return
    if (tab && TABS.some(t => t.id === tab)) {
      setActiveTab(tab)
      setVisited(v => (v.has(tab) ? v : new Set(v).add(tab)))
    }
    setSearchParams({}, { replace: true })
    if (section) {
      // Esperar a que se dibuje la pestaña
      setTimeout(() => document.getElementById(section)?.scrollIntoView({ behavior: 'smooth', block: 'start' }), 100)
    }
  }, [searchParams, setSearchParams])

  const isAdmin = authUser?.role === 'admin'
  const viaBridge = useViaBridge()

  useEffect(() => {
    localStorage.setItem('labnas-settings-tab', activeTab)
  }, [activeTab])

  // Por el puente (internet) la administracion no esta disponible
  const visibleTabs = TABS.filter(t => !t.adminOnly || (isAdmin && !viaBridge))

  const pane = (id: TabId, content: ReactNode, adminOnly = true) =>
    visited.has(id) && (!adminOnly || isAdmin) && (
      <div key={id} hidden={activeTab !== id} className="space-y-8">{content}</div>
    )

  return (
    <div className="space-y-8 max-w-4xl">
      {/* Tab bar */}
      <div
        className="flex flex-wrap gap-1 p-1 rounded-xl sticky top-0 z-10"
        style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
      >
        {visibleTabs.map(t => {
          const Icon = t.icon
          const active = activeTab === t.id
          return (
            <button
              key={t.id}
              onClick={() => openTab(t.id)}
              className="flex items-center gap-2 px-3 py-2 rounded-lg text-sm font-medium transition-all duration-200"
              style={{
                backgroundColor: active ? 'var(--accent)' : 'transparent',
                color: active ? '#ffffff' : 'var(--text-secondary)',
              }}
            >
              <Icon size={16} />
              <span>{t.label}</span>
            </button>
          )
        })}
      </div>

      {pane('general', <GeneralTab />, false)}
      {pane('lab', <LabTab />)}
      {pane('users', <UsersTab />)}
      {pane('notifications', <NotificationsTab />)}
      {pane('network', <NetworkTab />)}
      {pane('media', <MediaTab />)}
      {pane('system', <SystemTab />)}
      {pane('backups', <BackupsTab />)}
      {pane('admin', <AdminTab />)}
    </div>
  )
}
