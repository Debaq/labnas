export type ThemeName = 'dracula' | 'light' | 'nord' | 'solarized'

interface ThemeColors {
  'bg-primary': string
  'bg-secondary': string
  'bg-tertiary': string
  'text-primary': string
  'text-secondary': string
  accent: string
  'accent-hover': string
  'accent-alpha': string
  border: string
  'sidebar-bg': string
  'sidebar-text': string
  'sidebar-active': string
  'card-bg': string
  'card-border': string
  success: string
  'success-alpha': string
  warning: string
  danger: string
  'danger-alpha': string
  'input-bg': string
  'input-border': string
}

export const themes: Record<ThemeName, ThemeColors> = {
  dracula: {
    'bg-primary': '#282a36',
    'bg-secondary': '#1e1f29',
    'bg-tertiary': '#343746',
    'text-primary': '#f8f8f2',
    'text-secondary': '#6272a4',
    accent: '#bd93f9',
    'accent-hover': '#caa8ff',
    'accent-alpha': 'rgba(189, 147, 249, 0.12)',
    border: '#44475a',
    'sidebar-bg': '#21222c',
    'sidebar-text': '#6272a4',
    'sidebar-active': '#bd93f9',
    'card-bg': '#343746',
    'card-border': '#44475a',
    success: '#50fa7b',
    'success-alpha': 'rgba(80, 250, 123, 0.12)',
    warning: '#f1fa8c',
    danger: '#ff5555',
    'danger-alpha': 'rgba(255, 85, 85, 0.12)',
    'input-bg': '#44475a',
    'input-border': '#6272a4',
  },
  light: {
    'bg-primary': '#f8fafc',
    'bg-secondary': '#ffffff',
    'bg-tertiary': '#f1f5f9',
    'text-primary': '#1e293b',
    'text-secondary': '#64748b',
    accent: '#3b82f6',
    'accent-hover': '#2563eb',
    'accent-alpha': 'rgba(59, 130, 246, 0.10)',
    border: '#e2e8f0',
    'sidebar-bg': '#ffffff',
    'sidebar-text': '#64748b',
    'sidebar-active': '#3b82f6',
    'card-bg': '#ffffff',
    'card-border': '#e2e8f0',
    success: '#22c55e',
    'success-alpha': 'rgba(34, 197, 94, 0.10)',
    warning: '#eab308',
    danger: '#ef4444',
    'danger-alpha': 'rgba(239, 68, 68, 0.10)',
    'input-bg': '#ffffff',
    'input-border': '#cbd5e1',
  },
  nord: {
    'bg-primary': '#2e3440',
    'bg-secondary': '#272c36',
    'bg-tertiary': '#3b4252',
    'text-primary': '#eceff4',
    'text-secondary': '#81a1c1',
    accent: '#88c0d0',
    'accent-hover': '#8fbcbb',
    'accent-alpha': 'rgba(136, 192, 208, 0.12)',
    border: '#4c566a',
    'sidebar-bg': '#272c36',
    'sidebar-text': '#81a1c1',
    'sidebar-active': '#88c0d0',
    'card-bg': '#3b4252',
    'card-border': '#4c566a',
    success: '#a3be8c',
    'success-alpha': 'rgba(163, 190, 140, 0.12)',
    warning: '#ebcb8b',
    danger: '#bf616a',
    'danger-alpha': 'rgba(191, 97, 106, 0.12)',
    'input-bg': '#3b4252',
    'input-border': '#4c566a',
  },
  solarized: {
    'bg-primary': '#fdf6e3',
    'bg-secondary': '#eee8d5',
    'bg-tertiary': '#f5efdc',
    'text-primary': '#657b83',
    'text-secondary': '#93a1a1',
    accent: '#268bd2',
    'accent-hover': '#1a6fb5',
    'accent-alpha': 'rgba(38, 139, 210, 0.10)',
    border: '#eee8d5',
    'sidebar-bg': '#eee8d5',
    'sidebar-text': '#93a1a1',
    'sidebar-active': '#268bd2',
    'card-bg': '#eee8d5',
    'card-border': '#ddd6c1',
    success: '#859900',
    'success-alpha': 'rgba(133, 153, 0, 0.10)',
    warning: '#b58900',
    danger: '#dc322f',
    'danger-alpha': 'rgba(220, 50, 47, 0.10)',
    'input-bg': '#fdf6e3',
    'input-border': '#ddd6c1',
  },
}

export function applyTheme(name: ThemeName): void {
  const theme = themes[name]
  const root = document.documentElement
  for (const [key, value] of Object.entries(theme)) {
    root.style.setProperty(`--${key}`, value)
  }
}

export function getThemeNames(): ThemeName[] {
  return Object.keys(themes) as ThemeName[]
}
