export type ThemeName = 'dracula' | 'light' | 'nord' | 'solarized' | 'monokai' | 'catppuccin' | 'gruvbox' | 'tokyonight' | 'rosepine'

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
  monokai: {
    'bg-primary': '#272822',
    'bg-secondary': '#1e1f1c',
    'bg-tertiary': '#3e3d32',
    'text-primary': '#f8f8f2',
    'text-secondary': '#75715e',
    accent: '#a6e22e',
    'accent-hover': '#b6f23e',
    'accent-alpha': 'rgba(166, 226, 46, 0.12)',
    border: '#49483e',
    'sidebar-bg': '#1e1f1c',
    'sidebar-text': '#75715e',
    'sidebar-active': '#a6e22e',
    'card-bg': '#3e3d32',
    'card-border': '#49483e',
    success: '#a6e22e',
    'success-alpha': 'rgba(166, 226, 46, 0.12)',
    warning: '#e6db74',
    danger: '#f92672',
    'danger-alpha': 'rgba(249, 38, 114, 0.12)',
    'input-bg': '#3e3d32',
    'input-border': '#49483e',
  },
  catppuccin: {
    'bg-primary': '#1e1e2e',
    'bg-secondary': '#181825',
    'bg-tertiary': '#313244',
    'text-primary': '#cdd6f4',
    'text-secondary': '#6c7086',
    accent: '#cba6f7',
    'accent-hover': '#d4b8fa',
    'accent-alpha': 'rgba(203, 166, 247, 0.12)',
    border: '#45475a',
    'sidebar-bg': '#181825',
    'sidebar-text': '#6c7086',
    'sidebar-active': '#cba6f7',
    'card-bg': '#313244',
    'card-border': '#45475a',
    success: '#a6e3a1',
    'success-alpha': 'rgba(166, 227, 161, 0.12)',
    warning: '#f9e2af',
    danger: '#f38ba8',
    'danger-alpha': 'rgba(243, 139, 168, 0.12)',
    'input-bg': '#313244',
    'input-border': '#45475a',
  },
  gruvbox: {
    'bg-primary': '#282828',
    'bg-secondary': '#1d2021',
    'bg-tertiary': '#3c3836',
    'text-primary': '#ebdbb2',
    'text-secondary': '#928374',
    accent: '#fe8019',
    'accent-hover': '#f09030',
    'accent-alpha': 'rgba(254, 128, 25, 0.12)',
    border: '#504945',
    'sidebar-bg': '#1d2021',
    'sidebar-text': '#928374',
    'sidebar-active': '#fe8019',
    'card-bg': '#3c3836',
    'card-border': '#504945',
    success: '#b8bb26',
    'success-alpha': 'rgba(184, 187, 38, 0.12)',
    warning: '#fabd2f',
    danger: '#fb4934',
    'danger-alpha': 'rgba(251, 73, 52, 0.12)',
    'input-bg': '#3c3836',
    'input-border': '#504945',
  },
  tokyonight: {
    'bg-primary': '#1a1b26',
    'bg-secondary': '#16161e',
    'bg-tertiary': '#24283b',
    'text-primary': '#c0caf5',
    'text-secondary': '#565f89',
    accent: '#7aa2f7',
    'accent-hover': '#89b4fa',
    'accent-alpha': 'rgba(122, 162, 247, 0.12)',
    border: '#3b4261',
    'sidebar-bg': '#16161e',
    'sidebar-text': '#565f89',
    'sidebar-active': '#7aa2f7',
    'card-bg': '#24283b',
    'card-border': '#3b4261',
    success: '#9ece6a',
    'success-alpha': 'rgba(158, 206, 106, 0.12)',
    warning: '#e0af68',
    danger: '#f7768e',
    'danger-alpha': 'rgba(247, 118, 142, 0.12)',
    'input-bg': '#24283b',
    'input-border': '#3b4261',
  },
  rosepine: {
    'bg-primary': '#191724',
    'bg-secondary': '#1f1d2e',
    'bg-tertiary': '#26233a',
    'text-primary': '#e0def4',
    'text-secondary': '#6e6a86',
    accent: '#c4a7e7',
    'accent-hover': '#d4b7f7',
    'accent-alpha': 'rgba(196, 167, 231, 0.12)',
    border: '#393552',
    'sidebar-bg': '#1f1d2e',
    'sidebar-text': '#6e6a86',
    'sidebar-active': '#c4a7e7',
    'card-bg': '#26233a',
    'card-border': '#393552',
    success: '#9ccfd8',
    'success-alpha': 'rgba(156, 207, 216, 0.12)',
    warning: '#f6c177',
    danger: '#eb6f92',
    'danger-alpha': 'rgba(235, 111, 146, 0.12)',
    'input-bg': '#26233a',
    'input-border': '#393552',
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
