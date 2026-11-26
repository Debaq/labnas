import { api, jsonOrThrow } from './client'
import { finJson, type BudgetCategory, type FinFileRef } from './finance'

// --- Presupuesto, ejecucion, bienes, hitos, caja chica (handlers/finance/budget.rs) ---

export type BudgetLevel = 'ok' | 'alerta' | 'tope'

export interface ProjectConfig {
  project_id: string
  title: string
  currency: string
  responsible: string
  alert_pct: number
  closed_at: string | null
  configured: boolean
}

export interface ItemExec {
  id: string
  project_id: string
  category: BudgetCategory
  name: string
  approved: number
  notes: string
  committed: number
  spent: number
  available: number
  /** (comprometido + gastado) / aprobado en %; null si no hay monto aprobado */
  pct: number | null
  level: BudgetLevel
}

export interface Totals {
  approved: number
  committed: number
  spent: number
  available: number
  pct: number | null
}

export interface CategoryExec extends Totals {
  category: BudgetCategory
}

export interface ProjectExec {
  config: ProjectConfig
  items: ItemExec[]
  by_category: CategoryExec[]
  totals: Totals
  /** Comprometido o gastado que aun no se asigno a un item */
  unassigned: Totals
}

export interface SummaryRow {
  project_id: string
  title: string
  currency: string
  responsible: string
  closed_at: string | null
  items: number
  totals: Totals
  level: BudgetLevel
}

export interface ItemInput {
  project_id?: string
  category: BudgetCategory
  name: string
  approved: number
  notes: string
  /** Obligatorio al cambiar el monto aprobado */
  reason?: string
}

export interface ItemDetail {
  item: ItemExec
  changes: { old_amount: number; new_amount: number; reason: string; changed_by: string; changed_at: string; ledger_seq: number | null }[]
  commitments: { source: string; source_id: string; label: string; committed: number; spent: number; updated_at: string }[]
}

export interface AssetRow {
  id: string
  name: string
  project_id: string
  inventory_number: string
  responsible: string
  location: string
  status: string
  quantity: number
  unit_cost: number
  total_cost: number
  purchase_date: string
}

export interface Assets {
  items: AssetRow[]
  total_cost: number
  usage: { machine: string; sessions: number; hours: number }[]
  usage_hours: number
}

export interface Fund {
  id: string
  project_id: string
  title: string
  name: string
  responsible: string
  arqueo_days: number
  balance: number
  last_count_at: string | null
  last_difference: number | null
  next_arqueo: string
  arqueo_overdue: boolean
  closed: boolean
}

export interface PettyMove {
  id: string
  kind: 'ingreso' | 'egreso'
  amount: number
  concept: string
  moved_on: string
  budget_item_id: string | null
  item_name: string | null
  file: FinFileRef | null
  created_by: string
  created_at: string
  ledger_seq: number | null
  annulled_at: string | null
  annulled_by: string | null
  annul_reason: string | null
}

export interface PettyCount {
  id: string
  counted: number
  expected: number
  difference: number
  notes: string
  counted_by: string
  counted_at: string
  ledger_seq: number | null
}

export interface FundDetail {
  fund: Fund
  moves: PettyMove[]
  counts: PettyCount[]
}

export interface Income {
  id: string
  project_id: string
  amount: number
  origin: string
  destination: string
  received_on: string
  notes: string
  file: FinFileRef | null
  created_by: string
  created_at: string
  ledger_seq: number | null
  annulled_at: string | null
  annulled_by: string | null
  annul_reason: string | null
}

const enc = encodeURIComponent
const q = (project: string | null) => (project ? `?project_id=${enc(project)}` : '')

export async function fetchProjectConfig(pid: string): Promise<ProjectConfig> {
  return jsonOrThrow(await api(`/api/finance/projects/${enc(pid)}/config`), 'Error al leer la configuracion')
}

export async function saveProjectConfig(
  pid: string,
  body: { responsible: string; alert_pct: number; currency: string; closed?: boolean; reason?: string },
): Promise<ProjectConfig> {
  return jsonOrThrow(await api(`/api/finance/projects/${enc(pid)}/config`, finJson(body, 'PUT')), 'Error al guardar la configuracion')
}

export async function fetchExecution(pid: string): Promise<ProjectExec> {
  return jsonOrThrow(await api(`/api/finance/execution/${enc(pid)}`), 'Error al leer la ejecucion')
}

export async function fetchExecutionSummary(): Promise<SummaryRow[]> {
  return jsonOrThrow(await api('/api/finance/execution'), 'Error al leer el resumen')
}

export async function createBudgetItem(body: ItemInput): Promise<ItemExec> {
  return jsonOrThrow(await api('/api/finance/budget/items', finJson(body)), 'Error al crear el item')
}

export async function updateBudgetItem(id: string, body: ItemInput): Promise<ItemExec> {
  return jsonOrThrow(await api(`/api/finance/budget/items/${enc(id)}`, finJson(body, 'PUT')), 'Error al guardar el item')
}

export async function deleteBudgetItem(id: string): Promise<void> {
  const res = await api(`/api/finance/budget/items/${enc(id)}`, { method: 'DELETE' })
  if (!res.ok) throw new Error((await res.text()) || 'Error al borrar el item')
}

export async function fetchItemDetail(id: string): Promise<ItemDetail> {
  return jsonOrThrow(await api(`/api/finance/budget/items/${enc(id)}/detail`), 'Error al leer el item')
}

export async function fetchAssets(project: string | null): Promise<Assets> {
  return jsonOrThrow(await api(`/api/finance/assets${q(project)}`), 'Error al leer los bienes')
}

export async function fetchFunds(project: string | null): Promise<Fund[]> {
  return jsonOrThrow(await api(`/api/finance/petty/funds${q(project)}`), 'Error al leer las cajas chicas')
}

export async function createFund(body: { project_id: string; name: string; responsible: string; arqueo_days: number }): Promise<Fund> {
  return jsonOrThrow(await api('/api/finance/petty/funds', finJson(body)), 'Error al abrir la caja chica')
}

export async function updateFund(id: string, body: { name: string; responsible: string; arqueo_days: number }): Promise<Fund> {
  return jsonOrThrow(await api(`/api/finance/petty/funds/${enc(id)}`, finJson(body, 'PUT')), 'Error al guardar la caja chica')
}

export async function fetchFundDetail(id: string): Promise<FundDetail> {
  return jsonOrThrow(await api(`/api/finance/petty/funds/${enc(id)}`), 'Error al leer la caja chica')
}

export async function createPettyMove(
  fundId: string,
  body: { kind: 'ingreso' | 'egreso'; amount: number; concept: string; moved_on?: string; budget_item_id?: string | null; file?: string | null; file_name?: string },
): Promise<PettyMove> {
  return jsonOrThrow(await api(`/api/finance/petty/funds/${enc(fundId)}/moves`, finJson(body)), 'Error al registrar el movimiento')
}

export async function annulPettyMove(id: string, reason: string): Promise<void> {
  const res = await api(`/api/finance/petty/moves/${enc(id)}/annul`, finJson({ reason }))
  if (!res.ok) throw new Error((await res.text()) || 'Error al anular el movimiento')
}

export async function createPettyCount(fundId: string, body: { counted: number; notes: string }): Promise<PettyCount> {
  return jsonOrThrow(await api(`/api/finance/petty/funds/${enc(fundId)}/counts`, finJson(body)), 'Error al registrar el arqueo')
}

export async function fetchIncomes(project: string | null): Promise<Income[]> {
  return jsonOrThrow(await api(`/api/finance/incomes${q(project)}`), 'Error al leer los ingresos')
}

export async function createIncome(body: {
  project_id: string
  amount: number
  origin: string
  destination: string
  received_on?: string
  notes: string
  file?: string | null
  file_name?: string
}): Promise<Income> {
  return jsonOrThrow(await api('/api/finance/incomes', finJson(body)), 'Error al registrar el ingreso')
}

export async function annulIncome(id: string, reason: string): Promise<void> {
  const res = await api(`/api/finance/incomes/${enc(id)}/annul`, finJson({ reason }))
  if (!res.ok) throw new Error((await res.text()) || 'Error al anular el ingreso')
}
