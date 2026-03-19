import { useEffect, useState, useRef, type ChangeEvent } from 'react'
import {
  FolderOpen,
  FileText,
  FileImage,
  FileCode,
  FileArchive,
  File as FileIcon,
  Download,
  Trash2,
  Upload,
  FolderPlus,
  ChevronRight,
  Home,
  Loader2,
  Monitor,
  FileTextIcon,
  ImageIcon,
  Music,
  Video,
  Code,
  Globe,
  Layout,
  HardDrive,
  Disc,
  Trash,
  Printer,
  X,
  type LucideIcon,
} from 'lucide-react'
import { fetchFiles, uploadFile, downloadFile, deleteFile, createDirectory, fetchQuickAccess, fetchCupsPrinters, printFilePath } from '../api'
import type { FileEntry, QuickAccess, CupsPrinter } from '../types'

const iconMap: Record<string, LucideIcon> = {
  home: Home,
  monitor: Monitor,
  'file-text': FileTextIcon,
  download: Download,
  image: ImageIcon,
  music: Music,
  video: Video,
  code: Code,
  globe: Globe,
  layout: Layout,
  'hard-drive': HardDrive,
  disc: Disc,
  trash: Trash,
}

function getFileIcon(entry: FileEntry) {
  if (entry.is_dir) return <FolderOpen size={20} style={{ color: 'var(--warning)' }} />
  const ext = entry.extension?.toLowerCase()
  if (!ext) return <FileIcon size={20} style={{ color: 'var(--text-secondary)' }} />
  if (['png', 'jpg', 'jpeg', 'gif', 'svg', 'webp'].includes(ext))
    return <FileImage size={20} style={{ color: 'var(--accent)' }} />
  if (['ts', 'tsx', 'js', 'jsx', 'py', 'rs', 'go', 'html', 'css', 'json'].includes(ext))
    return <FileCode size={20} style={{ color: 'var(--success)' }} />
  if (['zip', 'tar', 'gz', 'rar', '7z'].includes(ext))
    return <FileArchive size={20} style={{ color: 'var(--danger)' }} />
  if (['txt', 'md', 'pdf', 'doc', 'docx'].includes(ext))
    return <FileText size={20} style={{ color: 'var(--accent)' }} />
  return <FileIcon size={20} style={{ color: 'var(--text-secondary)' }} />
}

function formatBytes(bytes: number): string {
  if (bytes === 0) return '0 B'
  const k = 1024
  const sizes = ['B', 'KB', 'MB', 'GB', 'TB']
  const i = Math.floor(Math.log(bytes) / Math.log(k))
  return parseFloat((bytes / Math.pow(k, i)).toFixed(2)) + ' ' + sizes[i]
}

function formatDate(dateStr: string): string {
  try {
    const date = new Date(dateStr)
    return date.toLocaleDateString('es-ES', {
      day: '2-digit',
      month: 'short',
      year: 'numeric',
      hour: '2-digit',
      minute: '2-digit',
    })
  } catch {
    return dateStr
  }
}

export default function FilesPage() {
  const [files, setFiles] = useState<FileEntry[]>([])
  const [currentPath, setCurrentPath] = useState('/')
  const [pathInput, setPathInput] = useState('/')
  const [loading, setLoading] = useState(true)
  const [uploading, setUploading] = useState(false)
  const [showNewFolder, setShowNewFolder] = useState(false)
  const [newFolderName, setNewFolderName] = useState('')
  const [quickAccess, setQuickAccess] = useState<QuickAccess[]>([])
  const [cupsPrinters, setCupsPrinters] = useState<CupsPrinter[]>([])
  const [printModal, setPrintModal] = useState<{ path: string; name: string } | null>(null)
  const [printPrinter, setPrintPrinter] = useState('')
  const [printCopies, setPrintCopies] = useState(1)
  const [printingFile, setPrintingFile] = useState(false)
  const fileInputRef = useRef<HTMLInputElement>(null)

  useEffect(() => {
    fetchQuickAccess().then(setQuickAccess).catch(() => {})
    fetchCupsPrinters().then((p) => {
      setCupsPrinters(p)
      const def = p.find((pr) => pr.is_default)
      if (def) setPrintPrinter(def.name)
      else if (p.length > 0) setPrintPrinter(p[0].name)
    }).catch(() => {})
  }, [])

  useEffect(() => {
    setPathInput(currentPath)
  }, [currentPath])

  async function loadFiles(path: string = currentPath) {
    setLoading(true)
    try {
      const data = await fetchFiles(path)
      setFiles(data)
    } catch (err) {
      console.error('Error cargando archivos:', err)
      setFiles([])
    } finally {
      setLoading(false)
    }
  }

  useEffect(() => {
    loadFiles()
  }, [currentPath])

  function navigateTo(path: string) {
    setCurrentPath(path)
  }

  function handleFolderClick(entry: FileEntry) {
    if (entry.is_dir) {
      setCurrentPath(entry.path)
    }
  }

  async function handleUpload(e: ChangeEvent<HTMLInputElement>) {
    const file = e.target.files?.[0]
    if (!file) return
    setUploading(true)
    try {
      await uploadFile(file, currentPath)
      await loadFiles()
    } catch (err) {
      console.error('Error subiendo archivo:', err)
    } finally {
      setUploading(false)
      if (fileInputRef.current) fileInputRef.current.value = ''
    }
  }

  async function handleDelete(path: string) {
    if (!confirm('Estas seguro de eliminar este archivo?')) return
    try {
      await deleteFile(path)
      await loadFiles()
    } catch (err) {
      console.error('Error eliminando:', err)
    }
  }

  async function handleCreateFolder() {
    if (!newFolderName.trim()) return
    const fullPath = currentPath === '/' ? `/${newFolderName}` : `${currentPath}/${newFolderName}`
    try {
      await createDirectory(fullPath)
      setNewFolderName('')
      setShowNewFolder(false)
      await loadFiles()
    } catch (err) {
      console.error('Error creando carpeta:', err)
    }
  }

  const printableExts = ['pdf', 'ps', 'eps', 'txt', 'text', 'log', 'png', 'jpg', 'jpeg', 'gif', 'tiff', 'tif', 'bmp', 'svg']

  function isPrintable(entry: FileEntry): boolean {
    if (entry.is_dir) return false
    const ext = entry.extension?.toLowerCase()
    return ext ? printableExts.includes(ext) : false
  }

  async function handlePrint() {
    if (!printModal || !printPrinter) return
    setPrintingFile(true)
    try {
      await printFilePath({
        path: printModal.path,
        printer: printPrinter,
        copies: printCopies,
      })
      setPrintModal(null)
    } catch (err) {
      console.error('Error imprimiendo:', err)
    } finally {
      setPrintingFile(false)
    }
  }

  const pathSegments = currentPath.split('/').filter(Boolean)

  return (
    <div className="space-y-6">
      {/* Path input bar */}
      <div className="flex items-center gap-2">
        <input
          type="text"
          value={pathInput}
          onChange={(e) => setPathInput(e.target.value)}
          onKeyDown={(e) => { if (e.key === 'Enter') { setCurrentPath(pathInput) } }}
          className="flex-1 px-3 py-1.5 rounded-lg text-sm font-mono outline-none"
          style={{
            backgroundColor: 'var(--input-bg)',
            color: 'var(--text-primary)',
            border: '1px solid var(--input-border)',
          }}
        />
        <button
          onClick={() => setCurrentPath(pathInput)}
          className="px-3 py-1.5 rounded-lg text-sm"
          style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
        >
          Ir
        </button>
      </div>

      {/* Quick Access */}
      {quickAccess.length > 0 && (
        <div className="flex items-center gap-2 flex-wrap">
          {quickAccess.map((qa) => {
            const Icon = iconMap[qa.icon] || FolderOpen
            const isActive = currentPath === qa.path
            return (
              <button
                key={qa.path}
                onClick={() => setCurrentPath(qa.path)}
                className="flex items-center gap-2 px-3 py-1.5 rounded-lg text-xs font-medium transition-all duration-200 hover:-translate-y-0.5"
                style={{
                  backgroundColor: isActive ? 'var(--accent-alpha)' : 'var(--card-bg)',
                  color: isActive ? 'var(--accent)' : 'var(--text-secondary)',
                  border: `1px solid ${isActive ? 'var(--accent)' : 'var(--card-border)'}`,
                }}
              >
                <Icon size={14} />
                {qa.name}
              </button>
            )
          })}
        </div>
      )}

      {/* Top bar */}
      <div className="flex items-center justify-between flex-wrap gap-4">
        {/* Breadcrumbs */}
        <div className="flex items-center gap-1 text-sm flex-wrap">
          <button
            onClick={() => navigateTo('/')}
            className="flex items-center gap-1 px-2 py-1 rounded transition-all duration-200 hover:opacity-80"
            style={{ color: 'var(--accent)' }}
          >
            <Home size={16} />
            <span>Inicio</span>
          </button>
          {pathSegments.map((segment, i) => {
            const pathToHere = '/' + pathSegments.slice(0, i + 1).join('/')
            return (
              <div key={pathToHere} className="flex items-center gap-1">
                <ChevronRight size={14} style={{ color: 'var(--text-secondary)' }} />
                <button
                  onClick={() => navigateTo(pathToHere)}
                  className="px-2 py-1 rounded transition-all duration-200 hover:opacity-80"
                  style={{ color: i === pathSegments.length - 1 ? 'var(--text-primary)' : 'var(--accent)' }}
                >
                  {segment}
                </button>
              </div>
            )
          })}
        </div>

        {/* Actions */}
        <div className="flex items-center gap-3">
          <button
            onClick={() => setShowNewFolder(true)}
            className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
            style={{
              backgroundColor: 'var(--card-bg)',
              color: 'var(--text-primary)',
              border: '1px solid var(--border)',
            }}
          >
            <FolderPlus size={16} />
            Nueva Carpeta
          </button>
          <button
            onClick={() => fileInputRef.current?.click()}
            disabled={uploading}
            className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium transition-all duration-200 hover:opacity-90"
            style={{
              backgroundColor: 'var(--accent)',
              color: '#ffffff',
            }}
          >
            {uploading ? <Loader2 size={16} className="animate-spin" /> : <Upload size={16} />}
            {uploading ? 'Subiendo...' : 'Subir Archivo'}
          </button>
          <input
            ref={fileInputRef}
            type="file"
            className="hidden"
            onChange={handleUpload}
          />
        </div>
      </div>

      {/* New folder input */}
      {showNewFolder && (
        <div
          className="flex items-center gap-3 p-4 rounded-lg"
          style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
        >
          <input
            type="text"
            value={newFolderName}
            onChange={(e) => setNewFolderName(e.target.value)}
            onKeyDown={(e) => e.key === 'Enter' && handleCreateFolder()}
            placeholder="Nombre de la carpeta"
            className="flex-1 px-3 py-2 rounded-lg text-sm outline-none"
            style={{
              backgroundColor: 'var(--input-bg)',
              color: 'var(--text-primary)',
              border: '1px solid var(--input-border)',
            }}
            autoFocus
          />
          <button
            onClick={handleCreateFolder}
            className="px-4 py-2 rounded-lg text-sm font-medium"
            style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
          >
            Crear
          </button>
          <button
            onClick={() => { setShowNewFolder(false); setNewFolderName('') }}
            className="px-4 py-2 rounded-lg text-sm font-medium"
            style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
          >
            Cancelar
          </button>
        </div>
      )}

      {/* Files Table */}
      <div
        className="rounded-xl overflow-hidden"
        style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}
      >
        {loading ? (
          <div className="flex items-center justify-center py-16">
            <Loader2 size={32} className="animate-spin" style={{ color: 'var(--accent)' }} />
          </div>
        ) : files.length === 0 ? (
          <div className="text-center py-16">
            <FolderOpen size={48} className="mx-auto mb-4" style={{ color: 'var(--text-secondary)' }} />
            <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
              Esta carpeta esta vacia
            </p>
          </div>
        ) : (
          <table className="w-full">
            <thead>
              <tr style={{ borderBottom: '1px solid var(--border)' }}>
                <th
                  className="text-left px-6 py-3 text-xs font-medium uppercase tracking-wider"
                  style={{ color: 'var(--text-secondary)' }}
                >
                  Nombre
                </th>
                <th
                  className="text-left px-6 py-3 text-xs font-medium uppercase tracking-wider"
                  style={{ color: 'var(--text-secondary)' }}
                >
                  Tamano
                </th>
                <th
                  className="text-left px-6 py-3 text-xs font-medium uppercase tracking-wider"
                  style={{ color: 'var(--text-secondary)' }}
                >
                  Modificado
                </th>
                <th
                  className="text-right px-6 py-3 text-xs font-medium uppercase tracking-wider"
                  style={{ color: 'var(--text-secondary)' }}
                >
                  Acciones
                </th>
              </tr>
            </thead>
            <tbody>
              {files.map((entry) => (
                <tr
                  key={entry.path}
                  className="transition-all duration-200 hover:opacity-90 cursor-pointer"
                  style={{ borderBottom: '1px solid var(--border)' }}
                  onClick={() => entry.is_dir && handleFolderClick(entry)}
                >
                  <td className="px-6 py-3">
                    <div className="flex items-center gap-3">
                      {getFileIcon(entry)}
                      <span className="text-sm font-medium" style={{ color: 'var(--text-primary)' }}>
                        {entry.name}
                      </span>
                    </div>
                  </td>
                  <td className="px-6 py-3">
                    <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                      {entry.is_dir ? '--' : formatBytes(entry.size)}
                    </span>
                  </td>
                  <td className="px-6 py-3">
                    <span className="text-sm" style={{ color: 'var(--text-secondary)' }}>
                      {formatDate(entry.modified)}
                    </span>
                  </td>
                  <td className="px-6 py-3">
                    <div className="flex items-center justify-end gap-2">
                      {!entry.is_dir && (
                        <button
                          onClick={(e) => { e.stopPropagation(); downloadFile(entry.path) }}
                          className="p-1.5 rounded-lg transition-all duration-200 hover:opacity-80"
                          style={{ color: 'var(--accent)' }}
                          title="Descargar"
                        >
                          <Download size={16} />
                        </button>
                      )}
                      {isPrintable(entry) && cupsPrinters.length > 0 && (
                        <button
                          onClick={(e) => { e.stopPropagation(); setPrintModal({ path: entry.path, name: entry.name }); setPrintCopies(1) }}
                          className="p-1.5 rounded-lg transition-all duration-200 hover:opacity-80"
                          style={{ color: 'var(--accent)' }}
                          title="Imprimir"
                        >
                          <Printer size={16} />
                        </button>
                      )}
                      <button
                        onClick={(e) => { e.stopPropagation(); handleDelete(entry.path) }}
                        className="p-1.5 rounded-lg transition-all duration-200 hover:opacity-80"
                        style={{ color: 'var(--danger)' }}
                        title="Eliminar"
                      >
                        <Trash2 size={16} />
                      </button>
                    </div>
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
      {/* Print Modal */}
      {printModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div
            className="rounded-xl p-6 w-full max-w-sm mx-4"
            style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}
          >
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-base font-semibold" style={{ color: 'var(--text-primary)' }}>Imprimir</h3>
              <button onClick={() => setPrintModal(null)} style={{ color: 'var(--text-secondary)' }}>
                <X size={18} />
              </button>
            </div>
            <p className="text-sm mb-4 truncate" style={{ color: 'var(--text-secondary)' }}>{printModal.name}</p>
            <div className="space-y-3">
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Impresora</label>
                <select
                  value={printPrinter}
                  onChange={(e) => setPrintPrinter(e.target.value)}
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                >
                  {cupsPrinters.map((p) => (
                    <option key={p.name} value={p.name}>{p.description} {p.is_default ? '(default)' : ''}</option>
                  ))}
                </select>
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Copias</label>
                <input
                  type="number"
                  min={1}
                  max={100}
                  value={printCopies}
                  onChange={(e) => setPrintCopies(parseInt(e.target.value) || 1)}
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none"
                  style={{ backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }}
                />
              </div>
            </div>
            <div className="flex items-center justify-end gap-3 mt-5">
              <button
                onClick={() => setPrintModal(null)}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}
              >
                Cancelar
              </button>
              <button
                onClick={handlePrint}
                disabled={printingFile}
                className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium"
                style={{ backgroundColor: 'var(--accent)', color: '#ffffff' }}
              >
                {printingFile ? <Loader2 size={14} className="animate-spin" /> : <Printer size={14} />}
                {printingFile ? 'Imprimiendo...' : 'Imprimir'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
