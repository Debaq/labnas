import type { ComponentType, CSSProperties } from 'react'

/** Componente de icono (lucide-react o react-icons) */
export type IconComponent = ComponentType<{ size?: number | string; className?: string; style?: CSSProperties }>
