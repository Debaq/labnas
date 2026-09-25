/** Tipo de vista previa segun la extension del archivo */
const KINDS: Record<string, string[]> = {
  image: ['png', 'jpg', 'jpeg', 'gif', 'webp', 'bmp', 'svg', 'avif', 'ico'],
  video: ['mp4', 'webm', 'ogv', 'mov', 'm4v'],
  audio: ['mp3', 'wav', 'ogg', 'oga', 'flac', 'm4a', 'aac', 'opus'],
  pdf: ['pdf'],
  text: [
    'txt', 'md', 'log', 'csv', 'tsv', 'json', 'yaml', 'yml', 'toml', 'ini', 'conf', 'cfg', 'env',
    'sh', 'bash', 'py', 'rs', 'js', 'ts', 'tsx', 'jsx', 'c', 'h', 'cpp', 'hpp', 'java', 'go', 'rb',
    'php', 'html', 'htm', 'css', 'xml', 'sql', 'gcode', 'ino', 'm', 'r', 'tex', 'bib', 'srt',
  ],
}

export type PreviewKind = 'image' | 'video' | 'audio' | 'pdf' | 'text'

export function previewKind(name: string): PreviewKind | null {
  const ext = name.includes('.') ? name.split('.').pop()!.toLowerCase() : ''
  for (const [kind, exts] of Object.entries(KINDS)) {
    if (exts.includes(ext)) return kind as PreviewKind
  }
  return null
}
