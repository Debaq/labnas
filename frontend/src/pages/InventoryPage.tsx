import { useEffect, useState, useCallback } from 'react'
import {
  Package, Plus, Trash2, Pencil, X, Loader2, Search, Filter,
  ShoppingCart, MapPin, Tag, ChevronDown, ChevronUp, ExternalLink,
} from 'lucide-react'
import {
  fetchInventoryCategories, createInventoryCategory, updateInventoryCategory, deleteInventoryCategory,
  fetchInventoryItems, createInventoryItem, updateInventoryItem, deleteInventoryItem,
} from '../api'
import type { InventoryCategory, InventoryItem, ItemStatus } from '../types'

const STATUS_LABELS: Record<ItemStatus, { label: string; color: string }> = {
  activo: { label: 'Activo', color: 'var(--success)' },
  mantenimiento: { label: 'Mantenimiento', color: 'var(--warning)' },
  retirado: { label: 'Retirado', color: 'var(--text-secondary)' },
  agotado: { label: 'Agotado', color: 'var(--danger)' },
}

const FILAMENT_MATERIALS = ['PLA', 'ABS', 'PETG', 'TPU', 'Nylon', 'ASA', 'PC', 'PVA', 'HIPS', 'Resina']
const FILAMENT_DENSITIES: Record<string, number> = { PLA: 1.24, ABS: 1.04, PETG: 1.27, TPU: 1.21, Nylon: 1.14, ASA: 1.07, PC: 1.20, PVA: 1.23, HIPS: 1.04, Resina: 1.10 }

export default function InventoryPage() {
  const [categories, setCategories] = useState<InventoryCategory[]>([])
  const [items, setItems] = useState<InventoryItem[]>([])
  const [loading, setLoading] = useState(true)
  const [selectedCat, setSelectedCat] = useState<string | null>(null)
  const [searchQuery, setSearchQuery] = useState('')
  const [showItemModal, setShowItemModal] = useState(false)
  const [showCatModal, setShowCatModal] = useState(false)
  const [editingItem, setEditingItem] = useState<InventoryItem | null>(null)
  const [editingCat, setEditingCat] = useState<InventoryCategory | null>(null)
  const [expandedItem, setExpandedItem] = useState<string | null>(null)

  // Form state
  const [form, setForm] = useState<Partial<InventoryItem>>({})
  const [catForm, setCatForm] = useState<{ name: string; icon: string; description: string }>({ name: '', icon: '', description: '' })

  const loadData = useCallback(async () => {
    setLoading(true)
    try {
      const [cats, itms] = await Promise.all([fetchInventoryCategories(), fetchInventoryItems()])
      setCategories(cats)
      setItems(itms)
    } catch {} finally { setLoading(false) }
  }, [])

  useEffect(() => { loadData() }, [loadData])

  const isFilamentCategory = (catId: string) => {
    const cat = categories.find(c => c.id === catId)
    return cat?.name.toLowerCase().includes('filament') || cat?.name.toLowerCase().includes('filamento')
  }

  const filteredItems = items.filter(item => {
    if (selectedCat && item.category_id !== selectedCat) return false
    if (searchQuery) {
      const q = searchQuery.toLowerCase()
      return item.name.toLowerCase().includes(q) || item.description.toLowerCase().includes(q) || item.brand.toLowerCase().includes(q) || item.supplier.toLowerCase().includes(q)
    }
    return true
  })

  function openNewItem(categoryId?: string) {
    setEditingItem(null)
    setForm({ category_id: categoryId || selectedCat || '', quantity: 1, unit: 'unidad', status: 'activo' as ItemStatus, custom_fields: {} })
    setShowItemModal(true)
  }

  function openEditItem(item: InventoryItem) {
    setEditingItem(item)
    setForm({ ...item })
    setShowItemModal(true)
  }

  async function handleSaveItem() {
    if (!form.name?.trim() || !form.category_id) return
    try {
      if (editingItem) {
        await updateInventoryItem(editingItem.id, form)
      } else {
        await createInventoryItem(form)
      }
      setShowItemModal(false)
      await loadData()
    } catch (e: any) { alert(e.message) }
  }

  async function handleDeleteItem(id: string) {
    if (!confirm('Eliminar este item?')) return
    await deleteInventoryItem(id)
    await loadData()
  }

  function openNewCat() {
    setEditingCat(null)
    setCatForm({ name: '', icon: '', description: '' })
    setShowCatModal(true)
  }

  function openEditCat(cat: InventoryCategory) {
    setEditingCat(cat)
    setCatForm({ name: cat.name, icon: cat.icon, description: cat.description })
    setShowCatModal(true)
  }

  async function handleSaveCat() {
    if (!catForm.name.trim()) return
    try {
      if (editingCat) {
        await updateInventoryCategory(editingCat.id, catForm)
      } else {
        await createInventoryCategory(catForm)
      }
      setShowCatModal(false)
      await loadData()
    } catch (e: any) { alert(e.message) }
  }

  async function handleDeleteCat(id: string) {
    if (!confirm('Eliminar esta categoria? Los items no se eliminaran.')) return
    await deleteInventoryCategory(id)
    await loadData()
  }

  const catIcons = ['📦', '🔧', '🧪', '🖨️', '🧵', '💻', '🔬', '⚡', '🛠️', '📐', '🧊', '🏭', '💊', '🧲', '📏', '🔩']

  const inputStyle = { backgroundColor: 'var(--input-bg)', color: 'var(--text-primary)', border: '1px solid var(--input-border)' }

  const totalValue = filteredItems.reduce((sum, i) => sum + i.cost * i.quantity, 0)

  return (
    <div className="flex gap-0 h-full -m-8" style={{ minHeight: 'calc(100vh - 130px)' }}>
      {/* Left sidebar: categories */}
      <div className="flex flex-col h-full shrink-0" style={{ width: '240px', backgroundColor: 'var(--bg-secondary)', borderRight: '1px solid var(--border)' }}>
        <div className="p-3" style={{ borderBottom: '1px solid var(--border)' }}>
          <button onClick={openNewCat} className="w-full flex items-center justify-center gap-1.5 px-3 py-1.5 rounded-lg text-xs font-medium"
            style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            <Plus size={12} /> Nueva Categoria
          </button>
        </div>

        <div className="flex-1 overflow-y-auto">
          <button onClick={() => setSelectedCat(null)}
            className="w-full text-left px-4 py-2.5 text-sm font-medium transition-all"
            style={{ backgroundColor: !selectedCat ? 'var(--accent-alpha)' : 'transparent', color: !selectedCat ? 'var(--accent)' : 'var(--text-primary)', borderBottom: '1px solid var(--border)', borderLeft: !selectedCat ? '3px solid var(--accent)' : '3px solid transparent' }}>
            <Package size={14} className="inline mr-2" /> Todo ({items.length})
          </button>
          {categories.map(cat => {
            const count = items.filter(i => i.category_id === cat.id).length
            const isSelected = selectedCat === cat.id
            return (
              <div key={cat.id} className="flex items-center group" style={{ borderBottom: '1px solid var(--border)' }}>
                <button onClick={() => setSelectedCat(cat.id)}
                  className="flex-1 text-left px-4 py-2.5 text-sm transition-all"
                  style={{ backgroundColor: isSelected ? 'var(--accent-alpha)' : 'transparent', color: isSelected ? 'var(--accent)' : 'var(--text-primary)', borderLeft: isSelected ? '3px solid var(--accent)' : '3px solid transparent' }}>
                  <span className="mr-2">{cat.icon || '📦'}</span>{cat.name}
                  <span className="ml-1 text-[10px]" style={{ color: 'var(--text-secondary)' }}>({count})</span>
                </button>
                <div className="pr-2 flex gap-0.5 opacity-0 group-hover:opacity-100 transition-opacity">
                  <button onClick={() => openEditCat(cat)} className="p-1 rounded hover:opacity-80" style={{ color: 'var(--text-secondary)' }}><Pencil size={10} /></button>
                  <button onClick={() => handleDeleteCat(cat.id)} className="p-1 rounded hover:opacity-80" style={{ color: 'var(--danger)' }}><Trash2 size={10} /></button>
                </div>
              </div>
            )
          })}
        </div>

        {/* Summary */}
        <div className="p-3 text-center" style={{ borderTop: '1px solid var(--border)' }}>
          <div className="text-lg font-bold font-mono" style={{ color: 'var(--accent)' }}>${totalValue.toLocaleString()}</div>
          <div className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>Valor total ({filteredItems.length} items)</div>
        </div>
      </div>

      {/* Main content */}
      <div className="flex-1 overflow-y-auto p-6">
        {/* Search + Add */}
        <div className="flex items-center gap-3 mb-4">
          <div className="relative flex-1">
            <Search size={14} className="absolute left-3 top-1/2 -translate-y-1/2" style={{ color: 'var(--text-secondary)' }} />
            <input value={searchQuery} onChange={e => setSearchQuery(e.target.value)} placeholder="Buscar por nombre, marca, proveedor..."
              className="w-full pl-9 pr-3 py-2 rounded-lg text-sm outline-none"
              style={inputStyle} />
          </div>
          <button onClick={() => openNewItem()} className="flex items-center gap-2 px-4 py-2 rounded-lg text-sm font-medium shrink-0"
            style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
            <Plus size={16} /> Nuevo Item
          </button>
        </div>

        {loading ? (
          <div className="flex justify-center py-16"><Loader2 size={24} className="animate-spin" style={{ color: 'var(--accent)' }} /></div>
        ) : filteredItems.length === 0 ? (
          <div className="text-center py-16">
            <Package size={48} className="mx-auto mb-3" style={{ color: 'var(--text-secondary)', opacity: 0.3 }} />
            <p className="text-sm" style={{ color: 'var(--text-secondary)' }}>
              {items.length === 0 ? 'Agrega categorias y items para empezar' : 'Sin resultados'}
            </p>
          </div>
        ) : (
          <div className="space-y-2">
            {filteredItems.map(item => {
              const cat = categories.find(c => c.id === item.category_id)
              const st = STATUS_LABELS[item.status] || STATUS_LABELS.activo
              const isExpanded = expandedItem === item.id
              const isFilament = isFilamentCategory(item.category_id)
              return (
                <div key={item.id} className="rounded-xl transition-all duration-200"
                  style={{ backgroundColor: 'var(--card-bg)', border: '1px solid var(--card-border)' }}>
                  <div className="flex items-center gap-3 px-5 py-3 cursor-pointer" onClick={() => setExpandedItem(isExpanded ? null : item.id)}>
                    <span className="text-lg shrink-0">{cat?.icon || '📦'}</span>
                    <div className="flex-1 min-w-0">
                      <div className="flex items-center gap-2">
                        <span className="text-sm font-medium truncate" style={{ color: 'var(--text-primary)' }}>{item.name}</span>
                        {item.brand && <span className="text-[10px]" style={{ color: 'var(--text-secondary)' }}>{item.brand}</span>}
                        <span className="text-[10px] px-1.5 py-0.5 rounded-full" style={{ backgroundColor: st.color + '20', color: st.color }}>{st.label}</span>
                      </div>
                      <div className="flex items-center gap-3 text-[10px] mt-0.5" style={{ color: 'var(--text-secondary)' }}>
                        {cat && <span>{cat.name}</span>}
                        {item.location && <span><MapPin size={9} className="inline" /> {item.location}</span>}
                        {isFilament && item.filament_remaining != null && item.filament_weight && (
                          <span style={{ color: (item.filament_remaining / item.filament_weight) < 0.2 ? 'var(--danger)' : 'var(--success)' }}>
                            {item.filament_remaining.toFixed(0)}g / {item.filament_weight}g
                          </span>
                        )}
                      </div>
                    </div>
                    <div className="text-right shrink-0">
                      <div className="text-sm font-mono font-bold" style={{ color: 'var(--text-primary)' }}>
                        {item.quantity} {item.unit}
                      </div>
                      {item.cost > 0 && <div className="text-[10px] font-mono" style={{ color: 'var(--accent)' }}>${(item.cost * item.quantity).toLocaleString()}</div>}
                    </div>
                    <div className="flex items-center gap-1 shrink-0">
                      <button onClick={(e) => { e.stopPropagation(); openEditItem(item) }} className="p-1 rounded hover:opacity-80" style={{ color: 'var(--text-secondary)' }}><Pencil size={14} /></button>
                      <button onClick={(e) => { e.stopPropagation(); handleDeleteItem(item.id) }} className="p-1 rounded hover:opacity-80" style={{ color: 'var(--danger)' }}><Trash2 size={14} /></button>
                      {isExpanded ? <ChevronUp size={14} style={{ color: 'var(--text-secondary)' }} /> : <ChevronDown size={14} style={{ color: 'var(--text-secondary)' }} />}
                    </div>
                  </div>

                  {isExpanded && (
                    <div className="px-5 pb-4 pt-1 grid grid-cols-2 md:grid-cols-3 gap-x-6 gap-y-2 text-xs" style={{ borderTop: '1px solid var(--border)' }}>
                      {item.description && <div className="col-span-full" style={{ color: 'var(--text-primary)' }}>{item.description}</div>}
                      {item.brand && <div><span style={{ color: 'var(--text-secondary)' }}>Marca:</span> {item.brand}</div>}
                      {item.model_name && <div><span style={{ color: 'var(--text-secondary)' }}>Modelo:</span> {item.model_name}</div>}
                      {item.serial_number && <div><span style={{ color: 'var(--text-secondary)' }}>Serie:</span> <span className="font-mono">{item.serial_number}</span></div>}
                      {item.cost > 0 && <div><span style={{ color: 'var(--text-secondary)' }}>Costo unit.:</span> <span className="font-mono">${item.cost}</span></div>}
                      {item.supplier && (
                        <div>
                          <span style={{ color: 'var(--text-secondary)' }}>Proveedor:</span> {item.supplier}
                          {item.supplier_url && <a href={item.supplier_url} target="_blank" rel="noopener noreferrer" className="ml-1 inline"><ExternalLink size={10} style={{ color: 'var(--accent)' }} className="inline" /></a>}
                        </div>
                      )}
                      {item.purchase_date && <div><span style={{ color: 'var(--text-secondary)' }}>Compra:</span> {item.purchase_date}</div>}
                      {item.location && <div><span style={{ color: 'var(--text-secondary)' }}>Ubicacion:</span> {item.location}</div>}
                      {isFilament && (
                        <>
                          {item.material_type && <div><span style={{ color: 'var(--text-secondary)' }}>Material:</span> {item.material_type}</div>}
                          {item.filament_diameter && <div><span style={{ color: 'var(--text-secondary)' }}>Diametro:</span> {item.filament_diameter}mm</div>}
                          {item.filament_color && <div className="flex items-center gap-1"><span style={{ color: 'var(--text-secondary)' }}>Color:</span> <span className="w-3 h-3 rounded-full inline-block" style={{ backgroundColor: item.filament_color }} /> {item.filament_color}</div>}
                          {item.filament_density && <div><span style={{ color: 'var(--text-secondary)' }}>Densidad:</span> {item.filament_density} g/cm3</div>}
                        </>
                      )}
                      {item.notes && <div className="col-span-full" style={{ color: 'var(--text-secondary)', fontStyle: 'italic' }}>{item.notes}</div>}
                      {Object.entries(item.custom_fields || {}).map(([k, v]) => (
                        <div key={k}><span style={{ color: 'var(--text-secondary)' }}>{k}:</span> {v}</div>
                      ))}
                    </div>
                  )}
                </div>
              )
            })}
          </div>
        )}
      </div>

      {/* Item Modal */}
      {showItemModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="rounded-xl p-6 w-full max-w-lg mx-4 max-h-[90vh] overflow-y-auto"
            style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}>
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
                {editingItem ? 'Editar Item' : 'Nuevo Item'}
              </h3>
              <button onClick={() => setShowItemModal(false)} style={{ color: 'var(--text-secondary)' }}><X size={20} /></button>
            </div>

            <div className="space-y-3">
              <div className="grid grid-cols-2 gap-3">
                <div className="col-span-2">
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Nombre *</label>
                  <input value={form.name || ''} onChange={e => setForm(p => ({ ...p, name: e.target.value }))}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
                </div>
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Categoria *</label>
                  <select value={form.category_id || ''} onChange={e => setForm(p => ({ ...p, category_id: e.target.value }))}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer" style={inputStyle}>
                    <option value="">Seleccionar...</option>
                    {categories.map(c => <option key={c.id} value={c.id}>{c.icon} {c.name}</option>)}
                  </select>
                </div>
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Estado</label>
                  <select value={form.status || 'activo'} onChange={e => setForm(p => ({ ...p, status: e.target.value as ItemStatus }))}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none cursor-pointer" style={inputStyle}>
                    <option value="activo">Activo</option>
                    <option value="mantenimiento">Mantenimiento</option>
                    <option value="retirado">Retirado</option>
                    <option value="agotado">Agotado</option>
                  </select>
                </div>
              </div>

              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Descripcion</label>
                <textarea value={form.description || ''} onChange={e => setForm(p => ({ ...p, description: e.target.value }))}
                  rows={2} className="w-full px-3 py-2 rounded-lg text-sm outline-none resize-none" style={inputStyle} />
              </div>

              <div className="grid grid-cols-3 gap-3">
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Cantidad</label>
                  <input type="number" min={0} step="any" value={form.quantity ?? ''} onChange={e => setForm(p => ({ ...p, quantity: parseFloat(e.target.value) || 0 }))}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
                </div>
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Unidad</label>
                  <input value={form.unit || ''} onChange={e => setForm(p => ({ ...p, unit: e.target.value }))} placeholder="unidad, kg, m..."
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
                </div>
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Costo ($)</label>
                  <input type="number" min={0} step="any" value={form.cost ?? ''} onChange={e => setForm(p => ({ ...p, cost: parseFloat(e.target.value) || 0 }))}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
                </div>
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Marca</label>
                  <input value={form.brand || ''} onChange={e => setForm(p => ({ ...p, brand: e.target.value }))}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
                </div>
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Modelo</label>
                  <input value={form.model_name || ''} onChange={e => setForm(p => ({ ...p, model_name: e.target.value }))}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
                </div>
              </div>

              <div className="grid grid-cols-2 gap-3">
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Proveedor</label>
                  <input value={form.supplier || ''} onChange={e => setForm(p => ({ ...p, supplier: e.target.value }))}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
                </div>
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>URL proveedor</label>
                  <input value={form.supplier_url || ''} onChange={e => setForm(p => ({ ...p, supplier_url: e.target.value }))} placeholder="https://..."
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono" style={inputStyle} />
                </div>
              </div>

              <div className="grid grid-cols-3 gap-3">
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Ubicacion</label>
                  <input value={form.location || ''} onChange={e => setForm(p => ({ ...p, location: e.target.value }))} placeholder="Lab 3, Estante A"
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
                </div>
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>N. Serie</label>
                  <input value={form.serial_number || ''} onChange={e => setForm(p => ({ ...p, serial_number: e.target.value }))}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none font-mono" style={inputStyle} />
                </div>
                <div>
                  <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Fecha compra</label>
                  <input type="date" value={form.purchase_date || ''} onChange={e => setForm(p => ({ ...p, purchase_date: e.target.value }))}
                    className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
                </div>
              </div>

              {/* Filament-specific fields */}
              {form.category_id && isFilamentCategory(form.category_id) && (
                <div className="p-3 rounded-lg space-y-3" style={{ backgroundColor: 'var(--bg-tertiary)', border: '1px solid var(--border)' }}>
                  <span className="text-xs font-semibold" style={{ color: 'var(--accent)' }}>Datos de filamento</span>
                  <div className="grid grid-cols-3 gap-3">
                    <div>
                      <label className="block text-[10px] mb-0.5" style={{ color: 'var(--text-secondary)' }}>Material</label>
                      <select value={form.material_type || ''} onChange={e => {
                        const mat = e.target.value
                        setForm(p => ({ ...p, material_type: mat, filament_density: FILAMENT_DENSITIES[mat] || p.filament_density }))
                      }} className="w-full px-2 py-1.5 rounded-lg text-xs outline-none cursor-pointer" style={inputStyle}>
                        <option value="">Seleccionar</option>
                        {FILAMENT_MATERIALS.map(m => <option key={m} value={m}>{m}</option>)}
                      </select>
                    </div>
                    <div>
                      <label className="block text-[10px] mb-0.5" style={{ color: 'var(--text-secondary)' }}>Diametro (mm)</label>
                      <select value={form.filament_diameter || ''} onChange={e => setForm(p => ({ ...p, filament_diameter: parseFloat(e.target.value) || null }))}
                        className="w-full px-2 py-1.5 rounded-lg text-xs outline-none cursor-pointer" style={inputStyle}>
                        <option value="">-</option>
                        <option value="1.75">1.75</option>
                        <option value="2.85">2.85</option>
                      </select>
                    </div>
                    <div>
                      <label className="block text-[10px] mb-0.5" style={{ color: 'var(--text-secondary)' }}>Densidad (g/cm3)</label>
                      <input type="number" step="0.01" value={form.filament_density ?? ''} onChange={e => setForm(p => ({ ...p, filament_density: parseFloat(e.target.value) || null }))}
                        className="w-full px-2 py-1.5 rounded-lg text-xs outline-none" style={inputStyle} />
                    </div>
                  </div>
                  <div className="grid grid-cols-3 gap-3">
                    <div>
                      <label className="block text-[10px] mb-0.5" style={{ color: 'var(--text-secondary)' }}>Peso total (g)</label>
                      <input type="number" value={form.filament_weight ?? ''} onChange={e => {
                        const w = parseFloat(e.target.value) || null
                        setForm(p => ({ ...p, filament_weight: w, filament_remaining: p.filament_remaining ?? w }))
                      }} className="w-full px-2 py-1.5 rounded-lg text-xs outline-none" style={inputStyle} />
                    </div>
                    <div>
                      <label className="block text-[10px] mb-0.5" style={{ color: 'var(--text-secondary)' }}>Restante (g)</label>
                      <input type="number" value={form.filament_remaining ?? ''} onChange={e => setForm(p => ({ ...p, filament_remaining: parseFloat(e.target.value) || null }))}
                        className="w-full px-2 py-1.5 rounded-lg text-xs outline-none" style={inputStyle} />
                    </div>
                    <div>
                      <label className="block text-[10px] mb-0.5" style={{ color: 'var(--text-secondary)' }}>Color</label>
                      <input type="color" value={form.filament_color || '#ffffff'} onChange={e => setForm(p => ({ ...p, filament_color: e.target.value }))}
                        className="w-full h-7 rounded cursor-pointer" style={{ border: '1px solid var(--border)' }} />
                    </div>
                  </div>
                </div>
              )}

              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Notas</label>
                <textarea value={form.notes || ''} onChange={e => setForm(p => ({ ...p, notes: e.target.value }))}
                  rows={2} className="w-full px-3 py-2 rounded-lg text-sm outline-none resize-none" style={inputStyle} placeholder="Observaciones, comentarios..." />
              </div>
            </div>

            <div className="flex justify-end gap-3 mt-5">
              <button onClick={() => setShowItemModal(false)} className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>Cancelar</button>
              <button onClick={handleSaveItem} disabled={!form.name?.trim() || !form.category_id}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                {editingItem ? 'Guardar' : 'Crear'}
              </button>
            </div>
          </div>
        </div>
      )}

      {/* Category Modal */}
      {showCatModal && (
        <div className="fixed inset-0 z-50 flex items-center justify-center bg-black/50">
          <div className="rounded-xl p-6 w-full max-w-sm mx-4" style={{ backgroundColor: 'var(--bg-secondary)', border: '1px solid var(--border)' }}>
            <div className="flex items-center justify-between mb-4">
              <h3 className="text-lg font-semibold" style={{ color: 'var(--text-primary)' }}>
                {editingCat ? 'Editar Categoria' : 'Nueva Categoria'}
              </h3>
              <button onClick={() => setShowCatModal(false)} style={{ color: 'var(--text-secondary)' }}><X size={20} /></button>
            </div>
            <div className="space-y-3">
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Nombre</label>
                <input value={catForm.name} onChange={e => setCatForm(p => ({ ...p, name: e.target.value }))}
                  placeholder="Ej: Filamentos, Equipos, Reactivos..."
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Icono</label>
                <div className="flex flex-wrap gap-1.5">
                  {catIcons.map(icon => (
                    <button key={icon} type="button" onClick={() => setCatForm(p => ({ ...p, icon }))}
                      className="w-8 h-8 rounded-lg flex items-center justify-center text-lg transition-all"
                      style={{ backgroundColor: catForm.icon === icon ? 'var(--accent-alpha)' : 'var(--bg-tertiary)', border: catForm.icon === icon ? '2px solid var(--accent)' : '2px solid transparent' }}>
                      {icon}
                    </button>
                  ))}
                </div>
              </div>
              <div>
                <label className="block text-xs font-medium mb-1" style={{ color: 'var(--text-secondary)' }}>Descripcion</label>
                <input value={catForm.description} onChange={e => setCatForm(p => ({ ...p, description: e.target.value }))}
                  className="w-full px-3 py-2 rounded-lg text-sm outline-none" style={inputStyle} />
              </div>
            </div>
            <div className="flex justify-end gap-3 mt-5">
              <button onClick={() => setShowCatModal(false)} className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ color: 'var(--text-secondary)', border: '1px solid var(--border)' }}>Cancelar</button>
              <button onClick={handleSaveCat} disabled={!catForm.name.trim()}
                className="px-4 py-2 rounded-lg text-sm font-medium"
                style={{ backgroundColor: 'var(--accent)', color: '#fff' }}>
                {editingCat ? 'Guardar' : 'Crear'}
              </button>
            </div>
          </div>
        </div>
      )}
    </div>
  )
}
